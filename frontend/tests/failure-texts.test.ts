import { describe, expect, it } from 'vitest'
import type { BuildFailure } from '../src/bindings.ts'
import { detailed, explain } from '../src/failure-text.ts'
import texts from '../src/failure-texts.json' with { type: 'json' }

function failure(code: string, message: string): BuildFailure {
  return { code, message, phase: null, cancelled: false, retryable: false }
}

describe('failure text shown in the window', () => {
  // window_seams exercises Failure::of on real Rust errors against this catalogue.
  it.each(Object.entries(texts.sentences))('%s displays its Russian translation', (code, text) => {
    expect(explain(failure(code, 'English diagnostic'))).toBe(text)
    expect(text).toMatch(/[А-Яа-я]/)
    expect(text).not.toMatch(/[ёЁ—]/)
  })

  it('keeps the wire message for shell and unknown failures', () => {
    for (const code of ['busy', 'destination_missing', 'interrupted', 'future']) {
      expect(explain(failure(code, 'сообщение оболочки'))).toBe('сообщение оболочки')
      expect(detailed.has(code)).toBe(false)
    }
  })

  it('shows details only for translated failures', () => {
    for (const code of detailed) expect(texts.sentences).toHaveProperty(code)
  })
})
