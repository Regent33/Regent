// Ticking elapsed-seconds counter for the composer's activity timer — counts
// up from 0 while `active` is true (turn activity === 'running'), resets to
// undefined once it goes false.
import { useEffect, useRef, useState } from 'react';

export function useElapsedSeconds(active: boolean): number | undefined {
  const [seconds, setSeconds] = useState<number | undefined>(undefined);
  const startRef = useRef(0);

  useEffect(() => {
    if (!active) {
      setSeconds(undefined);
      return;
    }
    startRef.current = Date.now();
    setSeconds(0);
    const id = setInterval(() => {
      setSeconds(Math.floor((Date.now() - startRef.current) / 1000));
    }, 1000);
    return () => clearInterval(id);
  }, [active]);

  return seconds;
}
