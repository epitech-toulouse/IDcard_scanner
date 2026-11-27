import { describe, expect, it } from "vitest";
import { openInTagOrder, tagFor, tagLetter } from "./sessionTags";
import type { SessionRow } from "./types";

const row = (id: number, starts_at: string, state: SessionRow["state"] = "open") => ({ id, starts_at, state }) as SessionRow;

describe("repères des créneaux", () => {
  it("ordre : début puis id, créneaux ouverts seulement", () => {
    const rows = [row(3, "2026-11-17T14:00:00"), row(1, "2026-11-17T14:30:00"), row(2, "2026-11-17T14:00:00"), row(4, "2026-11-17T08:00:00", "closed")];
    expect(openInTagOrder(rows).map((r) => r.id)).toEqual([2, 3, 1]);
  });

  it("lettres et couleurs", () => {
    const open = openInTagOrder([row(5, "2026-11-17T14:00:00"), row(6, "2026-11-17T15:00:00")]);
    expect(tagFor(open, 6)).toEqual({ letter: "B", cls: "tag-1" });
    expect(tagFor(open, 9)).toEqual({ letter: "·", cls: "tag-none" });
    expect(tagFor(open, null).cls).toBe("tag-none");
    expect([tagLetter(0), tagLetter(25), tagLetter(26), tagLetter(27)]).toEqual(["A", "Z", "AA", "AB"]);
  });
});
