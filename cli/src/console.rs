//! Отказ консольной формы – целиком по-русски.
//!
//! До 24.09.2026 консоль печатала русскую оболочку вокруг английской середины:
//! «Ошибка: I/O error at …: Permission denied (os error 13)». `Display` у
//! [`ArunaError`] остается английским – его читают журналы, тесты и провод к
//! окну, – а человеку у терминала отказ пересказывается здесь: заголовок на
//! каждый вариант, причина системы по ее виду, а формулировки, которые ядро
//! пишет само (сверка нормализации, проверка пакета и назначения, держатель
//! блокировки, покрытие шрифтов), – по таблице [`PHRASES`].
//!
//! Таблица держится тестом, который читает исходники ядра: формулировка,
//! добавленная туда без перевода, роняет его, а не проходит в консоль
//! по-английски. Имена, пути, адреса и числа – данные, они не переводятся.

use aruna::error::ArunaError;
use aruna::job::Phase;

/// Этап, на котором остановлен прогон, словами.
pub fn phase(phase: Phase) -> &'static str {
    match phase {
        Phase::Obtaining => "получение архива",
        Phase::Parsing => "разбор архива",
        Phase::Exporting => "запись документов",
        Phase::Validating => "проверка пакета",
        Phase::Publishing => "публикация",
    }
}

/// Что случилось, одной строкой: вариант, а в нем – его данные.
///
/// `match` без `_`: новый вариант без заголовка здесь не соберется.
pub fn headline(err: &ArunaError) -> String {
    use ArunaError::*;
    match err {
        Network { url, .. } => format!("сетевой сбой при загрузке {url}"),
        Http { url, status, .. } => format!("сервер ответил HTTP {status} на запрос {url}"),
        Truncated { url, expected, got } => {
            format!("загрузка {url} оборвалась: ожидалось {expected} байт, пришло {got}")
        }
        Oversized { url, limit, got } => {
            format!("ответ {url} длиннее предела: остановлено на {got} байтах, предел {limit}")
        }
        FontMissing { path, covers } => format!(
            "шрифта {} нет на месте, а он рисует {}; приложение установлено не полностью",
            path.display(),
            translated(covers)
        ),
        FontAltered {
            path,
            expected,
            found,
        } => format!(
            "файл шрифта {} заменен или поврежден: ожидался SHA-256 {expected}, найден {found}",
            path.display()
        ),
        ChecksumMismatch { url, expected, got } => {
            format!("сумма архива {url} не сошлась: ожидался MD5 {expected}, получен {got}")
        }
        Zip(_) => "архив ZIP не читается".to_string(),
        EmptyArchive => "архив пуст или в нем нет XML-документов".to_string(),
        Cancelled { phase: stage } => format!("остановлено на этапе: {}", phase(*stage)),
        Io { path, .. } => format!("сбой ввода-вывода: {}", path.display()),
        Replace { path, scratch, .. } => format!(
            "не удалось заменить {}; новая опись готова и лежит в {}",
            path.display(),
            scratch.display()
        ),
        ExportCollision {
            group,
            fragment,
            first,
            second,
            path,
        } => format!(
            "совпадение имен в {group}: {fragment} ведет к {}, и на него претендуют и {first}, и {second}",
            path.display()
        ),
        ExportFolderCollision {
            first_group,
            second_group,
            first,
            second,
        } => format!(
            "папки групп {first_group} и {second_group} на этом диске – одна папка: в первую ложится {first}, во вторую – {second}"
        ),
        ArchiveDuplicateEntry { entry } => {
            format!("архив называет {entry} дважды; документ может встречаться в нем один раз")
        }
        ExportDocumentTooLarge { entry, limit } => {
            format!("{entry} больше предела в {limit} байт для одного документа")
        }
        ExportDistorted { entry, reason } => {
            let mut parts = reason.split(" | ");
            let mut text = format!(
                "{entry} исказился бы при нормализации: {}",
                translated(parts.next().unwrap_or_default())
            );
            for more in parts {
                match more.split_once(": ") {
                    Some((name, why)) => {
                        text.push_str(&format!("\n  то же с {name}: {}", translated(why)))
                    }
                    None => text.push_str(&format!("\n  то же: {}", translated(more))),
                }
            }
            text
        }
        ExportIncomplete { expected, written } => {
            format!("записано {written} документов, а размечено {expected}")
        }
        ArchiveTooManyEntries { entries, limit } => {
            format!("в архиве {entries} записей, больше предела в {limit}")
        }
        ExportPackageTooLarge { written, limit } => {
            format!("пакет вырос до {written} байт, больше предела в {limit}")
        }
        ExportInvalid { root, count, first } => format!(
            "{} не прошел проверку: расхождений {count}, первое – {}",
            root.display(),
            // До десяти расхождений через «; » – каждое переводится само:
            // целиком строка не совпадет ни с одним шаблоном.
            first
                .split("; ")
                .map(translated)
                .collect::<Vec<_>>()
                .join("; ")
        ),
        PublishBusy { path, holder } => format!(
            "в эту папку публикует другой запуск: {} ({})",
            path.display(),
            translated(holder)
        ),
        ExportDestination { path, reason } => {
            format!("не заменяю {}: {}", path.display(), translated(reason))
        }
        DownloadsDir => "не удалось определить каталог Downloads".to_string(),
    }
}

/// Причина, которую назвала система или сеть, – по ее виду, а не ее словами.
pub fn cause(err: &ArunaError) -> Option<String> {
    match err {
        ArunaError::Io { source, .. } | ArunaError::Replace { source, .. } => Some(io(source)),
        ArunaError::Network { source, .. } => Some(network(source.as_ref())),
        ArunaError::Zip(inner) => Some(zip(inner)),
        _ => None,
    }
}

/// Системная ошибка словами, с ее кодом, если он есть.
fn io(err: &std::io::Error) -> String {
    use std::io::ErrorKind::*;
    let what = match err.kind() {
        NotFound => "нет такого файла или каталога",
        PermissionDenied => "нет прав",
        AlreadyExists => "такое имя уже занято",
        StorageFull => "на диске нет места",
        QuotaExceeded => "исчерпана квота на диске",
        ReadOnlyFilesystem => "диск доступен только для чтения",
        DirectoryNotEmpty => "каталог не пуст",
        NotADirectory => "это не каталог",
        IsADirectory => "это каталог, а не файл",
        FileTooLarge => "файл слишком велик",
        InvalidData => "данные не в том виде, какой ожидался",
        UnexpectedEof => "данные кончились раньше времени",
        TimedOut => "время ожидания истекло",
        Interrupted => "операция прервана",
        BrokenPipe => "читающая сторона закрыла канал",
        ConnectionRefused => "в соединении отказано",
        ConnectionReset => "соединение сброшено",
        ConnectionAborted => "соединение прервано",
        NotConnected => "соединения нет",
        ResourceBusy => "ресурс занят",
        OutOfMemory => "не хватило памяти",
        Unsupported => "операция не поддерживается",
        CrossesDevices => "переименование между разными дисками",
        _ => "сбой ввода-вывода",
    };
    match err.raw_os_error() {
        Some(code) => format!("{what} (код системы {code})"),
        None => what.to_string(),
    }
}

/// Сетевой отказ словами. Источник – ошибка `ureq`, системная ошибка или
/// собственная строка загрузчика; вид узнается по ним, текст не переносится.
fn network(source: &(dyn std::error::Error + 'static)) -> String {
    // Истекший срок – по виду системной ошибки где угодно в цепочке, раньше
    // текста: ureq 2 переводит EAGAIN в `TimedOut` только на своих чтениях, а
    // рукопожатие TLS читает голый сокет, и там истекший срок чтения на macOS –
    // EAGAIN (код 35) под словами «tls connection init failed». По тексту это
    // становилось «не удалось установить защищенное соединение» (заслон
    // 26.09.2026, замолчавший прокси).
    if waited_out(source) {
        return "сервер не ответил вовремя".into();
    }
    if let Some(err) = source.downcast_ref::<std::io::Error>() {
        return io(err);
    }
    // Прокси – по виду ошибки `ureq`, а не по тексту: с 24.09 запрос может
    // идти через прокси из окружения, и его отказ назывался «сетевым сбоем»,
    // а совет посылал проверять Zenodo (ревью 25.09).
    if let Some(ureq::Error::Transport(transport)) = source.downcast_ref::<ureq::Error>() {
        match transport.kind() {
            ureq::ErrorKind::ProxyConnect => return "прокси не пропустил соединение".into(),
            ureq::ErrorKind::ProxyUnauthorized => return "прокси не принял учетные данные".into(),
            ureq::ErrorKind::InvalidProxyUrl => return "адрес прокси записан неверно".into(),
            _ => {}
        }
    }
    let text = source.to_string();
    let lower = text.to_ascii_lowercase();
    let what = if lower.contains("ran past its deadline") {
        "попытка не уложилась в свой срок"
    } else if lower.contains("timed out") {
        "сервер не ответил вовремя"
    } else if lower.contains("connection refused") {
        "в соединении отказано"
    } else if lower.contains("dns") {
        "имя сервера не найдено"
    } else if lower.contains("reset") {
        "соединение сброшено"
    } else if lower.contains("too many redirects") {
        "слишком много перенаправлений"
    } else if lower.contains("closed before") || lower.contains("unexpected eof") {
        "сервер оборвал ответ"
    } else if lower.contains("tls") || lower.contains("certificate") {
        "не удалось установить защищенное соединение"
    } else {
        "сетевой сбой"
    };
    what.to_string()
}

/// Есть ли в цепочке ошибки системная ошибка истекшего срока. На сетевом
/// сокете `WouldBlock` значит ровно это: сокеты блокирующие, и EAGAIN
/// возвращается только по сроку чтения или записи.
fn waited_out(source: &(dyn std::error::Error + 'static)) -> bool {
    let mut next = Some(source);
    while let Some(err) = next {
        if let Some(io) = err.downcast_ref::<std::io::Error>() {
            if matches!(
                io.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) {
                return true;
            }
        }
        next = err.source();
    }
    false
}

/// Отказ библиотеки ZIP словами.
fn zip(err: &zip::result::ZipError) -> String {
    use zip::result::ZipError;
    match err {
        ZipError::Io(inner) => io(inner),
        ZipError::InvalidArchive(_) => "архив поврежден".to_string(),
        ZipError::UnsupportedArchive(_) => "формат архива не поддерживается".to_string(),
        ZipError::FileNotFound => "в архиве нет такого файла".to_string(),
        _ => "архив не читается".to_string(),
    }
}

/// Формулировки, которые ядро пишет само, и их перевод.
///
/// `{}` – место для данных: имени, пути, значения. Слева – текст ядра с
/// `{}` на месте каждой подстановки, справа – русский с теми же местами в том
/// же порядке.
pub const PHRASES: &[(&str, &str)] = &[
    // export::verify – сверка нормализации
    (
        "the source declares encoding=\"{}\" and the canonical declaration says UTF-8; the bytes would be kept and their meaning changed",
        "источник объявляет encoding=\"{}\", а каноническое объявление – UTF-8: байты остались бы прежними, а их смысл изменился",
    ),
    (
        "the source declares version=\"{}\" and the canonical declaration says 1.0; the bytes would be kept and their meaning changed",
        "источник объявляет version=\"{}\", а каноническое объявление – 1.0: байты остались бы прежними, а их смысл изменился",
    ),
    (
        "instruction <?{}…?> was dropped and is not on the permit list",
        "инструкция <?{}…?> снята, а в списке разрешенных ее нет",
    ),
    (
        "content differs at byte {} (source {} bytes, output {} bytes)\n      source: …{}…\n      output: …{}…",
        "содержимое расходится с байта {} (в источнике {} байт, на выходе {})\n      источник: …{}…\n      выход: …{}…",
    ),
    (
        "the output does not begin with the canonical declaration",
        "выход начинается не с канонического объявления",
    ),
    (
        "an instruction appears in the output that was not in the source",
        "на выходе появилась инструкция, которой в источнике не было",
    ),
    // export::validate – проверка пакета
    ("link is not a relative path inside the package: {}", "ссылка ведет не внутрь пакета: {}"),
    ("fragment link points at nothing: {}", "ссылка на документ ведет в пустоту: {}"),
    ("the inventory links something that is not a fragment: {}", "опись ссылается не на документ: {}"),
    ("placed but not linked: {}", "документ размещен, но в описи на него нет ссылки: {}"),
    ("linked but not placed: {}", "в описи есть ссылка, но нет документа: {}"),
    ("the package is missing {}", "в пакете нет {}"),
    ("orphan file in the package: {}", "в пакете лишний файл: {}"),
    ("expected document missing: {}", "нет ожидаемого документа: {}"),
    ("group without a folder: {}", "у группы нет папки: {}"),
    ("the manifest cannot be read: {}", "манифест не читается: {}"),
    (
        "{} is larger than the {} byte limit for this file, so it is not one this program wrote",
        "{} больше предела в {} байт для этого файла – значит, писала его не эта программа",
    ),
    ("the manifest has no {} entry for {}", "в манифесте нет записи {} для {}"),
    (
        "the manifest has a {} entry for {}, which is not in the package",
        "в манифесте есть запись {} для {}, а в пакете такого нет",
    ),
    (
        "the manifest lists {} distinct {} names for {} documents",
        "в манифесте {} разных имен {} на {} документов",
    ),
    ("{} is nested deeper than a package can be", "{} вложен глубже, чем бывает в пакете"),
    ("cannot read {}", "не читается {}"),
    (
        "the package holds a symbolic link, which an export never writes: {}",
        "в пакете символьная ссылка, а сборка их не пишет: {}",
    ),
    ("unexpected file in the package: {}", "в пакете неожиданный файл: {}"),
    // export::validate – назначение
    (
        "it is a symbolic link, and whatever it points at is not this exporter's to replace",
        "это символьная ссылка, и заменять то, на что она указывает, не дело сборщика",
    ),
    ("it is a file, not a package", "это файл, а не пакет"),
    ("there is no {} in it", "в нем нет {}"),
    (
        "it contains {}, which this exporter did not put there",
        "в нем лежит {}, которого сборщик туда не клал",
    ),
    // export – публикация
    (
        "the published package differs from the one that was built and checked",
        "опубликованный пакет отличается от собранного и проверенного",
    ),
    // export::lock – держатель блокировки и то, что стоит на ее месте
    (
        "something that is not a lock file stands where the publish lock belongs",
        "на месте блокировки публикации стоит не файл блокировки",
    ),
    ("pid {}, since {}", "процесс {}, с {}"),
    ("a lock file this program did not write", "файл блокировки, который эта программа не писала"),
    ("an unnamed run", "запуск без имени"),
    // fonts – что рисует шрифт
    ("U+100000, which nothing on a stock machine draws", "U+100000, которого не рисует ни один шрифт обычной машины"),
    ("the 376 cuneiform signs of the corpus", "376 клинописных знаков корпуса"),
    ("six editorial marks", "шесть редакторских помет"),
    ("the main face of the PDF", "основное начертание PDF"),
    ("the main face, italic", "основное начертание, курсив"),
    ("the main face, bold", "основное начертание, полужирный"),
    ("U+05C3", "U+05C3"),
    (
        "the terms the font beside it is distributed under",
        "условия, на которых распространяется шрифт рядом",
    ),
];

/// Перевод формулировки ядра; незнакомая остается как есть.
///
/// Незнакомой в выпуске быть не должно – это держит тест ниже, – но и
/// потерять текст отказа хуже, чем показать его по-английски.
pub fn translated(text: &str) -> String {
    PHRASES
        .iter()
        .find_map(|(english, russian)| fill(russian, holes(english, text)?))
        .unwrap_or_else(|| text.to_string())
}

/// Что стоит на местах `{}` шаблона `template` в тексте `text`, если текст
/// сделан по этому шаблону.
fn holes<'a>(template: &str, text: &'a str) -> Option<Vec<&'a str>> {
    let parts: Vec<&str> = template.split("{}").collect();
    let mut rest = text.strip_prefix(parts[0])?;
    let mut found = Vec::new();
    for (i, literal) in parts.iter().enumerate().skip(1) {
        let last = i == parts.len() - 1;
        let at = if last {
            if !rest.ends_with(literal) {
                return None;
            }
            rest.len() - literal.len()
        } else {
            rest.find(literal)?
        };
        found.push(&rest[..at]);
        rest = &rest[at + literal.len()..];
    }
    rest.is_empty().then_some(found)
}

/// `template` с подстановками `values` на местах `{}`, по порядку.
fn fill(template: &str, values: Vec<&str>) -> Option<String> {
    let parts: Vec<&str> = template.split("{}").collect();
    if parts.len() != values.len() + 1 {
        return None;
    }
    let mut out = parts[0].to_string();
    for (value, literal) in values.iter().zip(&parts[1..]) {
        out.push_str(value);
        out.push_str(literal);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Английские слова, которых в русском отказе быть не может. Данные –
    /// имена, пути, адреса – в примерах ниже выбраны так, чтобы в них этих слов
    /// не было, поэтому найденное слово – слово сообщения, а не данных.
    const ENGLISH: &[&str] = &[
        " the ",
        " is ",
        " was ",
        " and ",
        " not ",
        " of ",
        " for ",
        " in the ",
        "error",
        "failed",
        "could",
        "cannot",
        "refusing",
        "while",
        "which",
        "missing",
        "unnamed",
        "since",
        "declares",
        "package",
        "archive",
        "download ",
        "downloading",
        "entry",
        "limit",
        "bytes",
    ];

    fn english_in(text: &str) -> Vec<&'static str> {
        let padded = format!(" {} ", text.to_lowercase());
        ENGLISH
            .iter()
            .copied()
            .filter(|word| padded.contains(word))
            .collect()
    }

    fn io_error() -> std::io::Error {
        std::io::Error::from_raw_os_error(13)
    }

    /// По одному значению каждого варианта, с данными без английских слов.
    fn every_variant() -> Vec<ArunaError> {
        let p = || PathBuf::from("/п/к");
        vec![
            ArunaError::Network {
                url: "http://х/а.zip".into(),
                source: Box::new(io_error()),
            },
            ArunaError::Http {
                url: "http://х/а.zip".into(),
                status: 503,
                retry_after: None,
            },
            ArunaError::Truncated {
                url: "http://х/а.zip".into(),
                expected: 10,
                got: 5,
            },
            ArunaError::Oversized {
                url: "http://х/а.zip".into(),
                limit: 10,
                got: 11,
            },
            ArunaError::FontMissing {
                path: p(),
                covers: "six editorial marks",
            },
            ArunaError::FontAltered {
                path: p(),
                expected: "аа",
                found: "бб".into(),
            },
            ArunaError::ChecksumMismatch {
                url: "http://х/а.zip".into(),
                expected: "аа".into(),
                got: "бб".into(),
            },
            ArunaError::Zip(zip::result::ZipError::FileNotFound),
            ArunaError::EmptyArchive,
            ArunaError::Cancelled {
                phase: Phase::Publishing,
            },
            ArunaError::Io {
                path: p(),
                source: io_error(),
            },
            ArunaError::Replace {
                path: p(),
                scratch: p(),
                source: io_error(),
            },
            ArunaError::ExportCollision {
                group: "Г".into(),
                fragment: "Ф".into(),
                first: "А".into(),
                second: "Б".into(),
                path: p(),
            },
            ArunaError::ExportFolderCollision {
                first_group: "Г".into(),
                second_group: "г".into(),
                first: "А".into(),
                second: "Б".into(),
            },
            ArunaError::ArchiveDuplicateEntry { entry: "А".into() },
            ArunaError::ExportDocumentTooLarge {
                entry: "А".into(),
                limit: 1,
            },
            ArunaError::ExportDistorted {
                entry: "А".into(),
                reason: "the source declares encoding=\"Л\" and the canonical declaration says UTF-8; \
                         the bytes would be kept and their meaning changed | Б: the source declares \
                         version=\"1.1\" and the canonical declaration says 1.0; the bytes would be \
                         kept and their meaning changed"
                    .into(),
            },
            ArunaError::ExportIncomplete {
                expected: 2,
                written: 1,
            },
            ArunaError::ArchiveTooManyEntries {
                entries: 2,
                limit: 1,
            },
            ArunaError::ExportPackageTooLarge {
                written: 2,
                limit: 1,
            },
            ArunaError::ExportInvalid {
                root: p(),
                count: 3,
                first: "fragment link points at nothing: ./Х".into(),
            },
            // Несколько расхождений через «; », как их сводит проверка пакета.
            ArunaError::ExportInvalid {
                root: p(),
                count: 2,
                first: "placed but not linked: ./Х; the manifest cannot be read: Ш".into(),
            },
            ArunaError::ExportInvalid {
                root: p(),
                count: 1,
                first: "content differs at byte 7 (source 9 bytes, output 8 bytes)\n      source: …Х…\n      output: …Ш…".into(),
            },
            ArunaError::PublishBusy {
                path: p(),
                holder: "an unnamed run".into(),
            },
            ArunaError::ExportDestination {
                path: p(),
                reason: "it contains \"Ш\", which this exporter did not put there".into(),
            },
            ArunaError::DownloadsDir,
        ]
    }

    /// **Ни один отказ консоли не несет английской фразы.**
    #[test]
    fn every_refusal_is_told_in_russian() {
        for err in every_variant() {
            let mut text = headline(&err);
            if let Some(why) = cause(&err) {
                text.push_str(&why);
            }
            let found = english_in(&text);
            assert!(found.is_empty(), "{err:?} → {text:?}: {found:?}");
        }
    }

    /// Ошибка-обертка с источником – так ureq отдает отказ рукопожатия TLS:
    /// «tls connection init failed» поверх системной ошибки сокета.
    #[derive(Debug)]
    struct Wrapped(std::io::Error);
    impl std::fmt::Display for Wrapped {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "Connection Failed: tls connection init failed: {}",
                self.0
            )
        }
    }
    impl std::error::Error for Wrapped {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    /// **Истекший срок под рукопожатием TLS называется сроком, а не отказом
    /// защищенного соединения.** ureq 2 переводит EAGAIN в `TimedOut` только
    /// на тех чтениях, которые оборачивает сам; рукопожатие читает голый сокет,
    /// и истекший срок чтения на macOS приходит как EAGAIN (код 35). Заслон
    /// 26.09.2026 увидел за замолчавшим прокси «не удалось установить
    /// защищенное соединение» – слово «tls» в тексте брало верх.
    #[test]
    fn a_deadline_under_the_tls_handshake_is_named_as_a_deadline() {
        for kind in [std::io::ErrorKind::WouldBlock, std::io::ErrorKind::TimedOut] {
            let wrapped = Wrapped(std::io::Error::from(kind));
            assert_eq!(network(&wrapped), "сервер не ответил вовремя", "{kind:?}");
            let bare = std::io::Error::from(kind);
            assert_eq!(network(&bare), "сервер не ответил вовремя", "{kind:?}");
        }
        // Настоящий отказ TLS – без истекшего срока в цепочке – остается собой.
        let refused = Wrapped(std::io::Error::other("invalid peer certificate"));
        assert_eq!(
            network(&refused),
            "не удалось установить защищенное соединение"
        );
    }

    /// Страж со своим отрицательным контролем: английский текст ядра, не
    /// прошедший через перевод, он замечает.
    #[test]
    fn the_english_guard_notices_english() {
        assert!(!english_in("I/O error at /п/к: Permission denied").is_empty());
        assert!(!english_in(&ArunaError::EmptyArchive.to_string()).is_empty());
    }

    /// Все пять разных многоместных случаев перевода, включая данные с
    /// двоеточием внутри.
    #[test]
    fn a_phrase_is_filled_in_with_its_data() {
        assert_eq!(
            translated("the manifest lists 3 distinct pdf names for 4 documents"),
            "в манифесте 3 разных имен pdf на 4 документов"
        );
        assert_eq!(
            translated("pid 12, since 1790000000.5"),
            "процесс 12, с 1790000000.5"
        );
        assert_eq!(
            translated("fragment link points at nothing: ./CTH%201/a:b.xml"),
            "ссылка на документ ведет в пустоту: ./CTH%201/a:b.xml"
        );
        assert_eq!(translated("никакой шаблон"), "никакой шаблон");
    }

    /// **Отказ прокси называется отказом прокси**, а не сетевым сбоем.
    #[test]
    fn a_proxy_refusal_is_named_as_one() {
        use std::io::{Read as _, Write as _};
        use std::net::TcpListener;
        // Прокси, который на CONNECT отвечает отказом.
        let refused_by = |status: &'static str| {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().expect("addr").port();
            let proxy = std::thread::spawn(move || {
                let (mut conn, _) = listener.accept().expect("accept");
                let mut buf = [0u8; 1024];
                let _ = conn.read(&mut buf);
                let _ = conn.write_all(format!("HTTP/1.1 {status}\r\n\r\n").as_bytes());
            });
            let agent = ureq::AgentBuilder::new()
                .proxy(ureq::Proxy::new(format!("http://127.0.0.1:{port}")).expect("proxy"))
                .build();
            let err = agent
                .get("https://aruna-proxy-test.invalid/")
                .call()
                .expect_err("the proxy lets nothing through");
            let _ = proxy.join();
            (network(&err), err.to_string())
        };
        let (told, err) = refused_by("502 Bad Gateway");
        assert_eq!(told, "прокси не пропустил соединение", "for {err}");
        let (told, err) = refused_by("407 Proxy Authentication Required");
        assert_eq!(told, "прокси не принял учетные данные", "for {err}");
    }

    /// Литералы в первом аргументе вызовов `names` в файле `source`.
    fn literals_after(source: &str, calls: &[&str]) -> Vec<String> {
        let code = source.split("#[cfg(test)]").next().unwrap_or(source);
        let mut found = Vec::new();
        for call in calls {
            let mut rest = code;
            while let Some(at) = rest.find(call) {
                rest = &rest[at + call.len()..];
                let trimmed = rest.trim_start();
                let Some(body) = trimmed.strip_prefix('"') else {
                    continue;
                };
                let mut literal = String::new();
                let mut chars = body.chars();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('"') => literal.push('"'),
                            Some('n') => literal.push('\n'),
                            Some('\n') => {
                                // строка продолжается: пробелы начала следующей не считаются
                                while chars.clone().next().is_some_and(char::is_whitespace) {
                                    chars.next();
                                }
                            }
                            Some(other) => {
                                literal.push('\\');
                                literal.push(other);
                            }
                            None => break,
                        },
                        _ => literal.push(c),
                    }
                }
                found.push(literal);
            }
        }
        found
    }

    /// Шаблон из литерала `format!`: каждая подстановка `{…}` – это `{}`.
    fn as_template(literal: &str) -> String {
        let mut out = String::new();
        let mut depth = 0;
        for c in literal.chars() {
            match c {
                '{' => {
                    if depth == 0 {
                        out.push_str("{}");
                    }
                    depth += 1;
                }
                '}' => depth -= 1,
                _ if depth == 0 => out.push(c),
                _ => {}
            }
        }
        out
    }

    /// **Каждая формулировка, которую ядро пишет в отказ, здесь переведена.**
    ///
    /// Читает исходники ядра, а не список, который мог бы отстать: новая
    /// формулировка без перевода роняет этот тест.
    #[test]
    fn every_phrase_the_core_writes_has_a_translation() {
        let sources = [
            // Каждый `format!` и каждый литерал в `Err(` этих двух файлов, а не
            // только известные формы вызова: 25.09 четыре формулировки прошли
            // мимо – `Err(distortion(…))`, `Err("…".into())` и `first: format!(`.
            (include_str!("export/verify.rs"), &["format!(", "Err("][..]),
            (
                include_str!("export/validate.rs"),
                &["format!(", "refuse(", "Err("][..],
            ),
            (
                include_str!("export/lock.rs"),
                &["Some(_) => ", "None => ", "reason:"][..],
            ),
            (include_str!("fonts.rs"), &["covers: "][..]),
            (include_str!("export/mod.rs"), &["first: "][..]),
        ];
        // Не отказы: команды для нормализатора, а не текст для человека.
        let not_refusals = ["DROP_PI {}"];
        let known: Vec<&str> = PHRASES
            .iter()
            .map(|(english, _)| *english)
            .chain(not_refusals)
            .collect();
        let mut missing = Vec::new();
        let mut seen = 0;
        for (source, calls) in sources {
            for literal in literals_after(source, calls) {
                seen += 1;
                let template = as_template(&literal);
                if !known.contains(&template.as_str()) {
                    missing.push(template);
                }
            }
        }
        assert!(seen >= 30, "the scan found only {seen} phrases");
        assert!(missing.is_empty(), "no Russian for: {missing:#?}");
    }
}
