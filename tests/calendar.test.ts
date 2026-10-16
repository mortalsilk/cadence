import { describe, expect, it } from 'vitest';
import { calendarDays, clock, localDate } from '../src/calendar';
describe('calendar', () => {
  it('shows twelve complete Monday-to-Sunday weeks containing today', () => {
    const days = calendarDays(0, new Date(2026, 9, 15, 12));
    expect(days).toHaveLength(84);
    expect(days[83]).toBe('2026-10-18');
    expect(new Date(`${days[0]}T12:00:00`).getDay()).toBe(1);
    expect(new Set(days).size).toBe(84);
  });
  it('navigates contiguous periods across year and DST boundaries', () => {
    const date = new Date(2026, 0, 2, 12);
    const current = calendarDays(0, date),
      previous = calendarDays(1, date);
    const end = new Date(`${previous[83]}T12:00:00`);
    end.setDate(end.getDate() + 1);
    expect(localDate(end)).toBe(current[0]);
  });
  it('keeps Sunday in its own week and does not wrap long timers', () => {
    expect(calendarDays(0, new Date(2026, 9, 18, 12))[83]).toBe('2026-10-18');
    expect(clock(100 * 3600000 + 61000)).toBe('100:01:01');
    expect(clock(-1)).toBe('00:00:00');
  });
});
