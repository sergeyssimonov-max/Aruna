describe('Aruna window', () => {
  it('opens and shows the interface', async () => {
    const button = await $('button')
    await expect(button).toExist()
    await button.click()
    await expect(button).toHaveText(expect.stringContaining('1'))
  })

  it('reaches the Tauri backend', async () => {
    const kind = await browser.tauri.execute(({ core }) => typeof core.invoke)
    expect(kind).toBe('function')
    const version = await browser.tauri.execute(({ core }) => core.invoke('plugin:app|version'))
    expect(version).toBe('0.1.0')
  })
})
