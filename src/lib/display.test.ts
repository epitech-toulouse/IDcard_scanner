import { describe, expect, it } from "vitest";
import { isProjected, nextOpeningAt, nextSession, projectionEnd } from "./display";
import type { Session } from "./types";

const mk = (id: number, start: string, end: string, state: Session["state"] = "planned", late = 10): Session => ({
  id,
  title: `C${id}`,
  room: null,
  starts_at: `2026-11-17T${start}:00`,
  ends_at: `2026-11-17T${end}:00`,
  late_after_min: late,
  state,
});
const at = (t: string) => `2026-11-17T${t}:00`;

describe("affichage automatique", () => {
  it("projette un créneau ouvert jusqu'à début + tolérance de retard (exclue)", () => {
    const s = mk(1, "08:00", "10:00", "open");
    expect(projectionEnd(s)).toBe(at("08:10"));
    expect(isProjected(s, at("07:50"))).toBe(true);
    expect(isProjected(s, at("08:09"))).toBe(true);
    expect(isProjected(s, at("08:10"))).toBe(false);
    expect(isProjected({ ...s, state: "planned" }, at("08:00"))).toBe(false);
  });

  it("ne dépasse pas la fin du créneau", () => {
    expect(projectionEnd(mk(1, "08:00", "08:05", "open", 30))).toBe(at("08:05"));
  });

  it("le prochain créneau ignore ceux dont la fenêtre est passée", () => {
    const list = [mk(2, "10:00", "12:00"), mk(1, "08:00", "10:00", "open"), mk(3, "09:00", "10:00", "closed")];
    expect(nextSession(list, at("08:05"))?.id).toBe(1);
    expect(nextSession(list, at("08:05"), 1)?.id).toBe(2);
    expect(nextSession(list, at("08:15"))?.id).toBe(2);
    expect(nextSession(list, at("10:30"))).toBeNull();
  });

  it("annonce l'heure d'ouverture automatique ou le début", () => {
    const s = mk(1, "08:00", "10:00");
    expect(nextOpeningAt(s, 15)).toBe(at("07:45"));
    expect(nextOpeningAt(s, null)).toBe(at("08:00"));
  });
});
