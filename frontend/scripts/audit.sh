#!/usr/bin/env bash
# Проверка уязвимостей npm-пакетов. --strict не ограничивает время аудита.
#
# Зачем ограничение. `pnpm audit` обращается к внешнему сервису рекомендаций,
# и 04.09.2026 его конечная точка быстрой проверки перестала отвечать при
# полностью работоспособной площадке: команда висела без конца, а с ней вставал
# весь пре-коммитный набор. Проверка, зависящая от чужого сервиса, не вправе
# блокировать работу молчанием.
#
# Почему не `timeout`. Его нет в macOS. `perl` есть всегда, и `alarm` дает тот
# же результат: по истечении срока процесс получает сигнал и завершается кодом
# 142 (или 14 у некоторых оболочек).
#
# Три исхода различаются по коду возврата, и это главное:
#   0        – проверка прошла с действующими проверенными исключениями;
#   14, 142  – сервис не ответил за отведенное время: предупреждаем, но не
#              останавливаем работу, потому что причина не в проекте;
#   иное     – уязвимости найдены либо команда отказала по существу: код
#              возвращается как есть, и набор проверок останавливается.
set -uo pipefail

# Все пути и конфигурация относятся к этому frontend, а не к cwd вызывающего.
cd -- "$(dirname -- "$0")/.." || exit 1

if [[ $# -gt 1 || ( $# -eq 1 && "$1" != "--strict" ) ]]; then
  echo 'Использование: audit.sh [--strict]' >&2
  exit 2
fi

# Решение владельца от 18.11.2026: только проверенное локальное исправление
# picomatch. Тесты сверяют точные хеши патчей и установленного кода, все активные
# копии и воспроизведение ReDoS. Отказ не может попасть в обход таймаута ниже.
# Исключение добавляется только во временную копию для этого запуска.
pnpm exec vitest run tests/picomatch-security.test.ts --project node --passWithNoTests=false
guard_code=$?
if [[ $guard_code -ne 0 ]]; then
  echo 'АУДИТ ОТКАЗАЛ: локальное исправление picomatch не подтверждено.' >&2
  exit "$guard_code"
fi

echo 'picomatch: проверенное локальное исправление GHSA-fc5q-39xc-vjpf учтено только в этом запуске.'

# В pnpm 11 флаг audit --ignore СОХРАНЯЕТ исключение в workspace. Поэтому
# штатный аудит читает тот же lock-файл и политику из временного каталога,
# куда добавлен ровно один подтвержденный идентификатор. Проект не меняется.
audit_dir=$(mktemp -d "${TMPDIR:-/tmp}/aruna-audit.XXXXXX") || exit 1
trap 'rm -rf -- "$audit_dir"' EXIT
node --input-type=module - "$audit_dir" <<'NODE'
import { copyFileSync, cpSync, existsSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
const target = process.argv[2]
const text = readFileSync('pnpm-workspace.yaml', 'utf8')
const marker = 'audit:\n  ignore:\n'
if (text.includes('GHSA-fc5q-39xc-vjpf') || text.split(marker).length !== 2) {
  throw new Error('Unexpected audit configuration; conditional exception refused')
}
for (const file of ['package.json', 'pnpm-lock.yaml']) copyFileSync(file, join(target, file))
if (existsSync('.npmrc')) copyFileSync('.npmrc', join(target, '.npmrc'))
cpSync('patches', join(target, 'patches'), { recursive: true })
writeFileSync(join(target, 'pnpm-workspace.yaml'),
  text.replace(marker, marker + '    - GHSA-fc5q-39xc-vjpf\n'))
NODE
prepare_code=$?
if [[ $prepare_code -ne 0 ]]; then exit "$prepare_code"; fi

if [[ ${1:-} == '--strict' ]]; then
  pnpm --dir "$audit_dir" audit
  exit $?
fi

LIMIT="${ARUNA_AUDIT_TIMEOUT:-30}"
if [[ ! $LIMIT =~ ^([1-9]|[12][0-9]|30)$ ]]; then
  echo 'ARUNA_AUDIT_TIMEOUT должен быть целым числом от 1 до 30 секунд.' >&2
  exit 2
fi

perl -e "alarm ${LIMIT}; exec @ARGV" pnpm --dir "$audit_dir" audit
code=$?

case "${code}" in
  0)
    exit 0
    ;;
  14 | 142)
    echo ""
    echo "АУДИТ НЕ ВЫПОЛНЕН: сервис рекомендаций не ответил за ${LIMIT} секунд."
    echo "Это внешний отказ, а не находка. Перед выпуском обязателен pnpm run audit:strict."
    exit 0
    ;;
  *)
    echo ""
    echo "АУДИТ ОТКАЗАЛ, код ${code}: уязвимости найдены либо проверка не смогла отработать."
    exit "${code}"
    ;;
esac
