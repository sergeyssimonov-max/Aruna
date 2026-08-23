import { render, screen } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import Counter from './Counter.svelte'

describe('Counter', () => {
  it('increments on click', async () => {
    render(Counter)
    const button = screen.getByRole('button')
    expect(button).toBeInTheDocument()
    await userEvent.click(button)
    expect(button).toHaveTextContent('1')
  })
})
