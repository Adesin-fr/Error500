import { describe, expect, it } from 'vitest'
import { formatBytes, formatDateTime, formatDuration, levelClass, relativeTime, shortId } from './format'

describe('formatDuration', () => {
  it.each([
    [0, '0s'],
    [45, '45s'],
    [125, '2m 05s'],
    [3725, '1h 02m'],
  ])('%i s -> %s', (seconds, expected) => {
    expect(formatDuration(seconds)).toBe(expected)
  })
})

describe('formatBytes', () => {
  it.each([
    [0, '0 B'],
    [512, '512 B'],
    [1536, '1.5 KB'],
    [5 * 1024 * 1024, '5.0 MB'],
  ])('%i -> %s', (bytes, expected) => {
    expect(formatBytes(bytes)).toBe(expected)
  })
})

describe('relativeTime', () => {
  const now = new Date('2023-11-14T12:00:00Z')
  it.each([
    ['2023-11-14T11:59:58Z', "à l'instant"],
    ['2023-11-14T11:55:00Z', 'il y a 5 min'],
    ['2023-11-14T09:00:00Z', 'il y a 3 h'],
    ['2023-11-12T12:00:00Z', 'il y a 2 j'],
  ])('%s -> %s', (iso, expected) => {
    expect(relativeTime(iso, now)).toBe(expected)
  })

  it('shows a date for old values', () => {
    expect(relativeTime('2023-06-01T12:00:00Z', now)).toBe('01/06/2023')
  })
})

describe('formatDateTime', () => {
  it('formats in the given time zone', () => {
    expect(formatDateTime('2023-11-14T22:13:20Z', 'UTC')).toBe('14/11/2023 22:13:20')
  })
})

describe('shortId', () => {
  it('keeps the first 8 characters', () => {
    expect(shortId('9ec79c33ec9942ab8353589fcb2e04dc')).toBe('9ec79c33')
  })
})

describe('levelClass', () => {
  it.each([
    ['fatal', 'fatal'],
    ['error', 'error'],
    ['warning', 'warning'],
    ['warn', 'warning'],
    ['info', 'info'],
    ['debug', 'debug'],
    ['trace', 'debug'],
    ['weird', 'info'],
  ])('%s -> %s', (level, expected) => {
    expect(levelClass(level)).toBe(expected)
  })
})
