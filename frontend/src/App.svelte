<script lang="ts">
  import { onDestroy } from 'svelte'
  import type { Stage } from './bindings'
  import { detailed, explain } from './failure-text'
  import { windowActions } from './window-ipc'
  import { WindowState } from './window-state.svelte'

  const state = new WindowState()
  const { subscribe, load, build, pickFolder, stop, reveal } = windowActions(state)
  onDestroy(subscribe())
  void load()
  const screen = $derived(state.screen)
  const progress = $derived(screen.kind === 'building' ? screen.progress : null)

  const CORPUS = 'Thesaurus Linguarum Hethaeorum Digitalis'

  /**
   * Имя каждой стадии по-русски – и записью по объединению, а не словарем со
   * строковым ключом.
   *
   * `Stage` приходит из `bindings.ts` объединением двадцати одного литерала, и
   * `Record<Stage, string>` обязывает назвать их все: стадия, добавленная в
   * ядре, роняет `pnpm check` здесь. Ветки «на все остальное» тут нет нарочно –
   * она и есть тот отказ, ради которого стадия объявлена перечислением, а не
   * строкой: она показала бы читателю пустое место вместо забытого имени.
   */
  const STAGES: Record<Stage, string> = {
    'cache-unusable': 'Кеш загрузок не годится – беру архив заново',
    'cached-archive-rejected': 'Архив из кеша не подошел – скачиваю заново',
    'archive-from-cache': 'Архив нашелся в кеше',
    'zenodo-notice': 'Hethitologie-Portal Mainz просит передать',
    'zenodo-unreachable': 'Hethitologie-Portal Mainz не отвечает',
    'proxy-unusable': 'Прокси из окружения не годится – соединяюсь напрямую',
    downloading: 'Скачиваю архив',
    'download-retrying': 'Повторяю загрузку',
    'archive-kept': 'Архив сохранен',
    parsing: 'Разбираю архив',
    'entries-skipped': 'Пропускаю посторонние записи архива',
    indexed: 'Составил указатель документов',
    'reading-headers': 'Читаю заголовки документов',
    'headers-read': 'Заголовки прочитаны',
    writing: 'Записываю документы',
    'writing-pdfs': 'Перевожу документы в PDF',
    'checking-package': 'Проверяю, что получилось',
    'waiting-for-publication': 'Жду, пока другой запуск закончит публикацию в эту папку',
    'publishing-without-lock':
      'Этот диск не держит блокировку публикации – публикую без защиты от одновременного запуска',
    'checking-published': 'Проверяю опубликованный пакет',
    'previous-package-left': 'Прежний пакет оставлен на месте',
  }

  /**
   * Разряды неразрывным пробелом, вручную.
   *
   * `toLocaleString` дал бы то же самое там, где ICU полон, и другое – там, где
   * он урезан: в jsdom, которым идут модульные тесты, и в WKWebView, которым
   * идет окно, разрядка разная. Число на экране не должно зависеть от того,
   * чем его смотрят.
   */
  function spaced(value: number): string {
    return String(value).replace(/\B(?=(\d{3})+(?!\d))/g, ' ')
  }

  const stage: string = $derived.by(() => (progress === null ? 'Начинаю' : STAGES[progress.stage]))

  /**
   * Доля, когда стадия назвала обе половины, и `null` во всех прочих случаях.
   *
   * Знаменателя может не быть: сервер не обязан объявлять длину загрузки. Ноль
   * знаменателем тоже не считается – доля от него не «сто процентов», а
   * ничего, и полосе в этом случае положено двигаться без деления.
   */
  const fraction: number | null = $derived.by(() => {
    if (progress === null || progress.done === null) {
      return null
    }
    const total = progress.total
    if (total === null || total === 0) {
      return null
    }
    return Math.min(100, Math.round((progress.done / total) * 100))
  })

  /**
   * Счет PDF-фазы, когда событие его несет, и `null` во всех прочих случаях.
   *
   * `built` приходит `null`, пока PDF-фаза не началась, и нулем в объявлении
   * ее стадии: строка поэтому стоит уже с объявления, как «Обработано – 0 из
   * N, PDF построено – 0». Дальше такты несут счет ядра.
   *
   * Одна производная величина вместо четырех проверок в разметке: ветка
   * читает уже суженные числа, и условие показа записано в одном месте.
   */
  const pdfTally: { done: number; total: number; built: number } | null = $derived.by(() => {
    if (
      progress === null ||
      progress.stage !== 'writing-pdfs' ||
      progress.built === null ||
      progress.done === null ||
      progress.total === null
    ) {
      return null
    }
    return { done: progress.done, total: progress.total, built: progress.built }
  })

  /**
   * Заголовок называет состояние, а не программу.
   *
   * До сегодняшнего дня он говорил «готова к работе» безусловно – и в окне,
   * которое только что не смогло ничего прочитать, тоже. Заголовок – первое,
   * что читают, и он обязан быть про то, что происходит.
   */
  const heading: string = $derived.by(() => {
    switch (screen.kind) {
      case 'reading':
        return `Смотрю, что уже собрано`
      case 'absent':
        return `Библиотеки ${CORPUS} здесь еще нет`
      case 'present':
        return `Библиотека ${CORPUS} готова к работе`
      case 'unreadable':
        return `О библиотеке ${CORPUS} сказать нечего`
      case 'building':
        return `Собираю библиотеку ${CORPUS}`
      case 'built':
        return `Библиотека ${CORPUS} собрана`
      case 'failed':
        return screen.failure.cancelled ? 'Остановлено' : 'Собрать не удалось'
    }
  })
</script>

<main>
  <div class="ready">
    <h1>{heading}</h1>

    {#if screen.kind === 'reading'}
      <p class="about">Читаю, что собрано…</p>
    {:else if screen.kind === 'absent'}
      <p class="about">
        Программа возьмет TLHdig с Hethitologie-Portal Mainz (запись на Zenodo), разложит корпус по
        папкам в загрузках и создаст опись – один файл HTML, который открывается в браузере без
        сети. Сборка занимает около двух минут.
      </p>
    {:else if screen.kind === 'present' || screen.kind === 'built'}
      {@const spread = screen.kind === 'present' ? screen.stats.spread : screen.spread}
      {@const markup = screen.markup}
      {#if screen.kind === 'present'}
        <p class="metrics">
          <span>Рукописей – <span class="count">{spaced(screen.stats.manuscripts)}</span></span>
          <span>Групп CTH – <span class="count">{spaced(screen.stats.groups)}</span></span>
        </p>
      {:else}
        <p class="metrics">
          <span>Документов – <span class="count">{spaced(screen.report.documents)}</span></span>
          <span>Групп CTH – <span class="count">{spaced(screen.report.groups)}</span></span>
          <span>PDF – <span class="count">{spaced(screen.report.pdfs.built)}</span></span>
        </p>
      {/if}
      <!--
        Разбивка стоит отдельной строкой: итоги выше – это ответ на вопрос
        «что собрано», а эти две – на вопрос «как оно устроено», который задают
        вторым. Строки нет вовсе, когда групп нет: нулями она сказала бы о
        пустом пакете больше, чем о нем известно.
      -->
      {#if spread?.largest}
        <p class="spread">
          <span>
            Самая большая группа – <span class="count">{spread.largest.label}</span>
            ({spaced(spread.largest.fragments)})
          </span>
          <span>
            Групп из одной рукописи –
            <span class="count">{spaced(spread.singletons)}</span>
          </span>
        </p>
      {/if}
      <!--
        Что разборщик сказал о документах – в своей обертке: `.ready`
        расставляет детей через 28 пикселей, которыми отделены друг от друга
        разные сообщения, а здесь строки одного – счетчик и, когда об этих
        документах есть что сказать, абзацы под ним. Класс `.markup` отделяет
        разметку от итогов выше, и на него опираются тесты. Веток у счетчика
        три – «прочитаны все», «столько-то не могут быть преобразованы в PDF»
        и, для пакета без счета модели, «программа не прочитала столько-то».

        Блока нет вовсе, когда сведений нет: пакет, собранный до 06.09.2026,
        секции `xml` в манифесте не несет, и отказ команды `corpus_xml` – это
        он и есть, а не поломка.

        Крупным числом стоит `objected_to` – множество, которое не принимает
        модель документа (`cli/src/document.rs`), и последствие «не попадут в
        PDF» есть только у него. 206 – классификация разборщика, и до
        13.09.2026 это число стояло здесь на его месте, с последствием,
        принадлежащим 223.

        Третья ветка – пакет, собранный 06.09–10.09.2026: секция `xml` в нем
        есть, а счета модели нет, и `objected_to` приходит нулем. 206 там
        показано под своим собственным вопросом – что программа не прочитала,
        – и без последствия для PDF: приписать его этому числу значило бы
        повторить ту самую подмену.
      -->
      {#if markup}
        <div class="markup">
          {#if markup.objected_to > 0}
            <p class="spread">
              <span>
                Не могут быть преобразованы в PDF –
                <span class="count">{spaced(markup.objected_to)}</span>
                из {spaced(markup.documents)}
              </span>
            </p>
            <p class="markup-about">
              Программа их не читает: разметка каждого нарушает правила XML или правила пространств
              имен.
            </p>
            <p class="markup-about">
              Все они лежат в пакете вместе с остальными и открываются как обычно – такая разметка
              мешает переводу в PDF, а не хранению.
              {#if screen.kind === 'built'}
                В описи ссылка на такой документ открывает его XML.
              {/if}
            </p>
          {:else if markup.unread === 0}
            <p class="spread">
              <span>
                Прочитаны все – <span class="count">{spaced(markup.documents)}</span>
              </span>
            </p>
          {:else}
            <p class="spread">
              <span>
                Программа не прочитала –
                <span class="count">{spaced(markup.unread)}</span>
                из {spaced(markup.documents)}
              </span>
            </p>
            <p class="markup-about">
              Разметка нарушена внутри тела документа – не сходятся теги, пропущен знак «=» в
              атрибуте, значение атрибута не закрыто до конца файла. Все они лежат в пакете вместе с
              остальными и открываются как обычно.
            </p>
          {/if}
        </div>
      {/if}
      {#if screen.kind === 'built'}
        {#if screen.report.pdfs.refused > (markup?.objected_to ?? 0)}
          <p class="spread">
            <span>
              Без PDF – <span class="count">{spaced(screen.report.pdfs.refused)}</span>: причины
              записаны в манифесте пакета
            </span>
          </p>
        {/if}
        <p class="where">Пакет – {screen.report.package}</p>
      {/if}
    {:else if screen.kind === 'building'}
      <p class="stage">{stage}</p>
      <!--
        Полоса объявлена `progressbar` и без `aria-valuenow`, когда доли нет:
        по ARIA это и означает «идет, но неизвестно сколько осталось», – ровно
        то, что происходит при загрузке, чью длину сервер не назвал.
      -->
      <div
        class="bar"
        role="progressbar"
        aria-label="Ход сборки"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={fraction ?? undefined}
      >
        {#if fraction === null}
          <div class="bar-waiting"></div>
        {:else}
          <div class="bar-fill" style="width: {fraction}%"></div>
        {/if}
      </div>
      <!--
        Строка стоит только в PDF-фазе: до нее `built` приходит `null`, а в
        объявлении стадии – нулем, так что строка появляется уже с объявления
        как «Обработано – 0 из N, PDF построено – 0». Числа – ядра как есть:
        `done` и `total` – сколько документов обработано и сколько всего
        записано в пакет, `built` – сколько из обработанных стали PDF. Решение
        4.1 от 18.10.2026.
      -->
      {#if pdfTally !== null}
        <p class="metrics">
          <span>
            Обработано – <span class="count">{spaced(pdfTally.done)}</span> из
            <span class="count">{spaced(pdfTally.total)}</span>, PDF построено –
            <span class="count">{spaced(pdfTally.built)}</span>
          </span>
        </p>
      {/if}
      {#if screen.manuscripts !== null}
        <p class="metrics">
          <span>Рукописей – <span class="count">{spaced(screen.manuscripts)}</span></span>
          {#if screen.groups !== null}
            <span>Групп CTH – <span class="count">{spaced(screen.groups)}</span></span>
          {/if}
        </p>
      {/if}
      {#if screen.note}
        <p class="note">{screen.note}</p>
      {/if}
    {:else if screen.kind === 'failed'}
      <p class="about">{explain(screen.failure)}</p>
      <!--
        Вторая строка – фраза ядра целиком, и стоит она у трех кодов из
        `detailed`. Класс `where` взят потому, что он уже набран младшим
        размером и ломает длинные слова: там стоит имя документа, а окно не
        меняет ширину и не прокручивается.
      -->
      {#if detailed.has(screen.failure.code)}
        <p class="where">{screen.failure.message}</p>
      {/if}
    {/if}

    <!--
      Мелкая неприятность стоит здесь, под телом состояния, а не вместо него:
      не открылась опись, не прочлись числа – окно сообщает, что смогло, а не
      заменяется сообщением об ошибке целиком.
    -->
    {#if state.trouble}
      <p class="failure">{state.trouble}</p>
    {/if}
  </div>

  <div class="foot">
    <div class="controls">
      {#if screen.kind === 'building'}
        <button
          type="button"
          class="control"
          data-testid="primary"
          disabled={state.stopping}
          onclick={() => void stop()}
        >
          {state.stopping ? 'Останавливаю…' : 'Отменить'}
        </button>
      {:else if screen.kind === 'present'}
        <!--
          Путь снимается с состояния здесь, а не читается внутри обработчика:
          обработчик – замыкание, и разбор объединения по `kind` внутрь него не
          доходит. `{@const}` стоит там, где ветка уже выбрана.
        -->
        {@const inventory = screen.inventory}
        <!--
          Описи может не быть и при целом пакете, и тогда предлагать ее незачем:
          главным действием становится пересборка – единственное, чем окно может
          эту опись вернуть.
        -->
        {#if inventory !== null}
          <button
            type="button"
            class="control"
            data-testid="primary"
            onclick={() => void reveal(inventory)}
          >
            Открыть опись
          </button>
          <button type="button" class="control control-quiet" onclick={() => void build(null)}>
            Пересобрать
          </button>
        {:else}
          <button
            type="button"
            class="control"
            data-testid="primary"
            onclick={() => void build(null)}
          >
            Пересобрать
          </button>
        {/if}
        <button type="button" class="control control-quiet" onclick={() => void pickFolder()}>
          Собрать в папку…
        </button>
      {:else if screen.kind === 'built'}
        {@const inventory = screen.report.inventory}
        <button
          type="button"
          class="control"
          data-testid="primary"
          onclick={() => void reveal(inventory)}
        >
          Открыть опись
        </button>
        <button type="button" class="control control-quiet" onclick={() => void build(null)}>
          Пересобрать
        </button>
        <button type="button" class="control control-quiet" onclick={() => void pickFolder()}>
          Собрать в папку…
        </button>
      {:else if screen.kind === 'failed' && screen.failure.cancelled}
        <!--
          У отмены ветка своя, и вот почему она нужна. Ядро метит любую отмену
          `retryable: false`, и это верно: `retryable` – совет о неисправности,
          а остановил прогон человек, неисправности не было. Окно же читало
          один этот флаг, и отмененный прогон уходил в ветку неповторимого
          отказа, где главное действие – `build(null)`. Папка, которую человек
          только что выбрал, забывалась, а тернарник `cancelled ? …` в соседней
          ветке был мертв: провод такой формы не выдает.

          Повтор повторяет тот же прогон – ту же запись и ту же папку, если ее
          выбирали. Подпись следует за поведением: «по умолчанию» стоит там и
          только там, где зовут `build(null)`.
        -->
        <button
          type="button"
          class="control"
          data-testid="primary"
          onclick={() => void build(state.destination)}
        >
          {state.destination === null ? 'Собрать по умолчанию' : 'Собрать'}
        </button>
        <button type="button" class="control control-quiet" onclick={() => void pickFolder()}>
          Собрать в папку…
        </button>
      {:else if screen.kind === 'failed' && screen.failure.retryable}
        <!--
          Повтор повторяет тот же прогон: ту же закрепленную запись Zenodo – и
          ту же папку, если ее выбирали. Кнопка, которая после отказа делает не
          то же самое, ответила бы не на тот вопрос.
        -->
        <button
          type="button"
          class="control"
          data-testid="primary"
          onclick={() => void build(state.destination)}
        >
          Повторить
        </button>
        <button type="button" class="control control-quiet" onclick={() => void pickFolder()}>
          Собрать в папку…
        </button>
      {:else if screen.kind === 'failed'}
        <!--
          Отказ, который не помечен `retryable`, второй попыткой обычно не
          лечится: тот же прогон кончится тем же. Пока у окна был выбор архива,
          он и стоял здесь главным действием – единственное, что можно было
          изменить. Источник теперь один, и назвать другой окну нечем.

          Кнопка все же стоит, и не для вида: переполненный диск, занятый
          каталог вывода, столкновение имен в нем – это чинится снаружи окна, и
          после починки нужно чем-то начать заново. Второе, что здесь можно
          изменить, не выходя из окна, – папка: рядом стоит ее выбор, и занятый
          каталог вывода обходится другим каталогом. Экран без единого действия
          был бы тупиком. Обещания успеха кнопка не дает, поэтому названа
          «Собрать по умолчанию», а не «Повторить».
        -->
        <button
          type="button"
          class="control"
          data-testid="primary"
          onclick={() => void build(null)}
        >
          Собрать по умолчанию
        </button>
        <button type="button" class="control control-quiet" onclick={() => void pickFolder()}>
          Собрать в папку…
        </button>
      {:else if screen.kind === 'unreadable'}
        {@const inventory = screen.inventory}
        <button
          type="button"
          class="control"
          data-testid="primary"
          onclick={() => void build(null)}
        >
          Собрать по умолчанию
        </button>
        {#if inventory !== null}
          <button
            type="button"
            class="control control-quiet"
            onclick={() => void reveal(inventory)}
          >
            Открыть опись
          </button>
        {/if}
        <button type="button" class="control control-quiet" onclick={() => void pickFolder()}>
          Собрать в папку…
        </button>
      {:else if screen.kind === 'absent'}
        <button
          type="button"
          class="control"
          data-testid="primary"
          onclick={() => void build(null)}
        >
          Собрать по умолчанию
        </button>
        <button type="button" class="control control-quiet" onclick={() => void pickFolder()}>
          Собрать в папку…
        </button>
      {/if}
    </div>
    {#if state.stopping}
      <p class="patience">
        Это может занять несколько секунд: запрос к Hethitologie-Portal Mainz и подсчет контрольной
        суммы архива прервать нельзя, сборка остановится на ближайшем документе.
      </p>
    {/if}
  </div>
</main>

<!--
  Стили этого блока – здесь, а не в `app.css`.

  Верстка окна лежит в `app.css` целиком, и правило это не отменяется: там
  палитра, шкала размеров и все, что делит между собой несколько экранов.
  Здесь только то, что живет ровно в одной ветке одного состояния и нигде
  больше, – и области видимости Svelte довольно, чтобы этого не объявлять
  глобально. Плоские селекторы и вложенности нет по той же причине, что и
  там: окно живет в WKWebView macOS 13, и вложенный CSS Safari 16 не понимает.
  Цвета и размеры взяты токенами и ступенями `app.css`, своих здесь нет.
-->
<style>
  /* Строки одного сообщения – в своей колонке с промежутком: 28 пикселей
   * `.ready` отделяют сообщения друг от друга, а не строки внутри одного.
   *
   * Своих отступов у пяти чисел нет, и это решение, а не недосмотр. `.ready`
   * расставляет их через 28 пикселей, и на опустевшем экране они читаются
   * группой сверху вниз сами: два итога, под ними разбивка, под ней разметка.
   * Сдвинуть их плотнее отсюда нечем и не нужно: `.metrics` и `.spread`
   * объявлены в `app.css`, а `.metrics` носят заодно строки прогона и отчета,
   * и правило в этом файле переставило бы заодно и их – ради экрана, о
   * котором его не просили. */
  .markup {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }

  /* Фраза о том, где эти документы лежат, – по мерке прозаических строк
   * экрана: та же ширина и выключка, что у заголовка. Класс свой, а не
   * `.about`: тот объявлен глобально и принадлежит строкам, которые сменяют
   * друг друга по состояниям, – эта же стоит рядом с числами. */
  .markup-about {
    max-width: 480px;
    margin: 0;
    text-align: center;
  }
</style>
