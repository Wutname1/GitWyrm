import { describe, expect, it } from 'vitest'
import { CommandError, failureLogLevel } from './commandError'
import { unwrap } from './queryKeys'

describe('failure log level', () => {
  // The backend already reported this failure; logging it at error again filed
  // a second crash report for every failure.
  it('never logs a backend failure at error', () => {
    expect(failureLogLevel('error', new CommandError('git push failed'))).toBe('warn')
  })

  it('keeps error for a failure that started in the frontend', () => {
    expect(failureLogLevel('error', new TypeError('x is undefined'))).toBe('error')
  })

  it('follows the classified severity below error', () => {
    expect(failureLogLevel('warning', new Error('x'))).toBe('warn')
    expect(failureLogLevel('info', new CommandError('x'))).toBe('info')
  })

  it('marks errors unwrapped from a command result', () => {
    try {
      unwrap({ status: 'error', error: 'nope' })
      expect.unreachable()
    } catch (e) {
      expect(e).toBeInstanceOf(CommandError)
      expect((e as Error).message).toBe('nope')
    }
  })
})
