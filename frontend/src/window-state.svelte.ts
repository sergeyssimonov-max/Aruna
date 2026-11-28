import type {
  BuildFailure,
  BuildProgress,
  BuildReport,
  CorpusStats,
  Spread,
  XmlSummary,
} from './bindings'

type Screen =
  | { kind: 'reading' }
  | { kind: 'absent' }
  | { kind: 'present'; stats: CorpusStats; inventory: string | null; markup: XmlSummary | null }
  | { kind: 'unreadable'; inventory: string | null }
  | {
      kind: 'building'
      previousInventory: string | null
      progress: BuildProgress | null
      manuscripts: number | null
      groups: number | null
      note: string | null
    }
  | { kind: 'built'; report: BuildReport; spread: Spread | null; markup: XmlSummary | null }
  | { kind: 'failed'; failure: BuildFailure }

export class WindowState {
  screen: Screen = $state({ kind: 'reading' })
  destination: string | null = $state(null)
  startupFontCheckFailed = $state(false)
  trouble: string | null = $state(null)
  edition: string | null = $state(null)
  stopping = $state(false)

  // Номер текущего задания приходит с первым событием. До него может
  // прийти запоздавшее событие уже завершенного задания.
  private job: number | null = null
  private finished: number | null = null

  begin(chosen: string | null, previousInventory: string | null): void {
    this.destination = chosen
    this.job = null
    this.trouble = null
    this.stopping = false
    this.screen = {
      kind: 'building',
      previousInventory,
      progress: null,
      manuscripts: null,
      groups: null,
      note: null,
    }
  }

  advance(event: BuildProgress): void {
    const screen = this.screen
    if (screen.kind !== 'building' || event.job === this.finished) return
    this.job ??= event.job
    if (event.job !== this.job) return
    screen.progress = event
    if (event.manuscripts !== null) screen.manuscripts = event.manuscripts
    if (event.groups !== null) screen.groups = event.groups
    if (event.note !== null) screen.note = event.note
  }

  finish(job: number | null = this.job): void {
    this.finished = job
  }
}
