import type { BuildFailure } from './bindings.ts'
import texts from './failure-texts.json' with { type: 'json' }

const sentences: Readonly<Record<string, string | undefined>> = texts.sentences
export const detailed: ReadonlySet<string> = new Set(texts.detailed)

const CANCELLED_ANYWHERE =
  'Остановлено. Незаконченное убрано, а прежний пакет, если он был, остался на месте.'

const CANCELLED: Record<string, string | undefined> = {
  obtaining: 'Остановлено во время загрузки архива. На диске ничего не изменилось.',
  parsing: 'Остановлено во время чтения архива. На диске ничего не изменилось.',
  exporting:
    'Остановлено во время записи пакета. Незаконченное убрано, а прежний пакет, если он был, остался на месте.',
  validating:
    'Остановлено во время проверки записанного. Незаконченное убрано, а прежний пакет, если он был, остался на месте.',
  publishing:
    'Остановлено перед самой заменой пакета. Прежний пакет, если он был, остался на месте.',
}

export function explain(failure: BuildFailure): string {
  if (!failure.cancelled) {
    return sentences[failure.code] ?? failure.message
  }
  return (failure.phase === null ? undefined : CANCELLED[failure.phase]) ?? CANCELLED_ANYWHERE
}
