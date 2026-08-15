import { useCallback, useEffect, useRef, useState } from "react";
import { OVERSCAN_PX } from "./virtual-list";

/** The vertical slice of the list that is currently mounted, in pixels. */
export type ScrollWindow = { y0: number; y1: number };

const INITIAL: ScrollWindow = { y0: 0, y1: 720 };

/** Fraction of the overscan that must be used up before the window moves. */
const HYSTERESIS = 0.45;

/**
 * Track which pixel range of a scroller should be rendered.
 *
 * Three things keep this from re-rendering on every scroll event, and each is
 * a separate concern that used to sit inline in the page component:
 *
 * * events are coalesced to one per animation frame;
 * * the window only moves once the viewport has eaten into the overscan
 *   margin, so a few pixels of scrolling do not re-render the list;
 * * the live scroll position lives in a ref, so reading it costs no render.
 *
 * It also publishes the scrollbar's width to the frame as `--vl-sb`: the
 * heading row sits outside the scrolling element and would otherwise be that
 * many pixels wider than the rows beneath it.
 *
 * @param scroller the element that scrolls
 * @param frame    the element carrying the `--vl-sb` custom property
 * @param ready    false while there is nothing to measure
 */
export function useScrollWindow(
  scroller: React.RefObject<HTMLElement | null>,
  frame: React.RefObject<HTMLElement | null>,
  ready: boolean,
): { range: ScrollWindow; resetWindow: () => void } {
  const [range, setRange] = useState<ScrollWindow>(INITIAL);
  const rangeRef = useRef<ScrollWindow>(INITIAL);
  const frameRef = useRef(0);

  const publish = useCallback((scrollTop: number, viewport: number) => {
    const prev = rangeRef.current;
    const margin = OVERSCAN_PX * HYSTERESIS;
    if (scrollTop >= prev.y0 + margin && scrollTop + viewport <= prev.y1 - margin) return;
    const next = {
      y0: scrollTop > OVERSCAN_PX ? scrollTop - OVERSCAN_PX : 0,
      y1: scrollTop + viewport + OVERSCAN_PX,
    };
    rangeRef.current = next;
    setRange(next);
  }, []);

  /** Back to the top — the list below is not the list that was there. */
  const resetWindow = useCallback(() => {
    const el = scroller.current;
    const next = { y0: 0, y1: (el?.clientHeight ?? 600) + OVERSCAN_PX };
    rangeRef.current = next;
    setRange(next);
    el?.scrollTo({ top: 0 });
  }, [scroller]);

  useEffect(() => {
    const el = scroller.current;
    if (!el || !ready) return;

    const onScroll = () => {
      if (frameRef.current) return;
      frameRef.current = requestAnimationFrame(() => {
        frameRef.current = 0;
        publish(el.scrollTop, el.clientHeight);
      });
    };

    const syncScrollbarWidth = () => {
      const width = el.offsetWidth - el.clientWidth;
      frame.current?.style.setProperty("--vl-sb", `${Math.max(0, width)}px`);
    };

    const ro = new ResizeObserver(() => {
      syncScrollbarWidth();
      publish(el.scrollTop, el.clientHeight);
    });
    ro.observe(el);
    syncScrollbarWidth();
    publish(el.scrollTop, el.clientHeight || 600);

    el.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      el.removeEventListener("scroll", onScroll);
      ro.disconnect();
      if (frameRef.current) cancelAnimationFrame(frameRef.current);
    };
  }, [scroller, frame, ready, publish]);

  return { range, resetWindow };
}
