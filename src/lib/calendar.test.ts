import { describe, expect, it } from "vitest";
import { layoutDay, weekHours } from "./calendar";

const r = (start: string, end: string) => ({ starts_at: `2026-11-17T${start}:00`, ends_at: `2026-11-17T${end}:00` });

describe("weekHours", () => {
  it("8 h – 18 h par défaut", () => {
    expect(weekHours([])).toEqual({ from: 8, to: 18 });
    expect(weekHours([r("09:00", "11:00")])).toEqual({ from: 8, to: 18 });
  });
  it("s'élargit aux créneaux tôt ou tard", () => {
    expect(weekHours([r("07:30", "09:00"), r("18:00", "19:15")])).toEqual({ from: 7, to: 20 });
  });
});

describe("layoutDay", () => {
  it("position verticale d'après l'heure", () => {
    const [a] = layoutDay([r("09:30", "11:00")], 8, 56);
    expect(a.top).toBe(1.5 * 56 + 2);
    expect(a.height).toBe(1.5 * 56 - 4);
    expect([a.left, a.width]).toEqual([0, 100]);
  });
  it("deux créneaux qui se chevauchent se partagent la colonne", () => {
    const [a, b] = layoutDay([r("09:00", "11:00"), r("10:00", "12:00")], 8, 56);
    expect([a.left, a.width]).toEqual([0, 50]);
    expect([b.left, b.width]).toEqual([50, 50]);
  });
  it("créneaux bout à bout : pleine largeur", () => {
    const out = layoutDay([r("09:00", "10:00"), r("10:00", "11:00")], 8, 56);
    expect(out.map((p) => p.width)).toEqual([100, 100]);
  });
  it("une piste libérée est réutilisée", () => {
    const [a, b, c] = layoutDay([r("09:00", "12:00"), r("09:00", "10:00"), r("10:30", "11:30")], 8, 56);
    expect(a.left).toBe(0);
    expect(b.left).toBe(50);
    expect(c.left).toBe(50);
  });
});
