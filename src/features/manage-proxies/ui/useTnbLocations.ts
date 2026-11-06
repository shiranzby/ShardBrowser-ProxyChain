import { useCallback, useEffect, useRef, useState } from "react";
import type { TnbLocationState } from "./TnbLocationPicker";

const EMPTY: TnbLocationState = { status: "idle", items: [] };

/** A result belongs to one provider and one parent. A late answer never changes another list. */
export function useTnbLocations(key: string, fetchLocations: () => Promise<TnbLocationState>) {
  const [current, setCurrent] = useState<{ key: string; state: TnbLocationState }>({ key, state: EMPTY });
  const sequence = useRef(0);
  const pending = useRef<{ key: string; sequence: number } | null>(null);
  useEffect(() => () => { sequence.current++; pending.current = null; }, [key]);
  const load = useCallback(() => {
    if (pending.current?.key === key) return;
    const asked = ++sequence.current;
    pending.current = { key, sequence: asked };
    setCurrent({ key, state: { status: "loading", items: [] } });
    void fetchLocations().then(
      (state) => { if (sequence.current === asked) setCurrent({ key, state }); },
      (error: unknown) => { if (sequence.current === asked) setCurrent({ key, state: { status: "error", items: [], errorCode: String(error instanceof Error ? error.message : error) } }); },
    ).finally(() => { if (pending.current?.sequence === asked) pending.current = null; });
  }, [key, fetchLocations]);
  return { state: current.key === key ? current.state : EMPTY, load };
}
