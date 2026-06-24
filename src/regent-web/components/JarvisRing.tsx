"use client";

import { Canvas, useFrame } from "@react-three/fiber";
import { useMemo, useRef } from "react";
import * as THREE from "three";

// The Jarvis "core" — a glowing teal ring (Regent's mark) that slowly turns and
// pulses with the call's loudness. three.js earns its place here: real depth +
// emissive bloom-ish glow the braille viz can't give.
interface Props {
  analyser: AnalyserNode | null;
  speaking?: boolean;
  className?: string;
}

function Core({ analyser, speaking }: { analyser: AnalyserNode | null; speaking: boolean }) {
  const ring = useRef<THREE.Mesh>(null);
  const halo = useRef<THREE.Mesh>(null);
  const lvl = useRef(0);
  const bins = useMemo(
    () => new Uint8Array(analyser ? analyser.frequencyBinCount : 0),
    [analyser],
  );

  useFrame((_, dt) => {
    let target = 0;
    if (analyser && bins.length) {
      analyser.getByteFrequencyData(bins);
      let s = 0;
      for (let i = 0; i < 32; i++) s += bins[i] ?? 0;
      target = s / 32 / 255;
    }
    lvl.current += (target - lvl.current) * Math.min(1, dt * 6); // smooth

    if (ring.current) {
      ring.current.rotation.z += dt * 0.25;
      ring.current.scale.setScalar(1 + lvl.current * 0.18);
      (ring.current.material as THREE.MeshStandardMaterial).emissiveIntensity =
        (speaking ? 1.4 : 0.8) + lvl.current * 1.8;
    }
    if (halo.current) {
      halo.current.rotation.z -= dt * 0.16;
      halo.current.scale.setScalar(1 + lvl.current * 0.1);
    }
  });

  return (
    <group>
      <mesh ref={ring}>
        <torusGeometry args={[1.6, 0.045, 24, 160]} />
        <meshStandardMaterial color="#2dd4bf" emissive="#5eead4" emissiveIntensity={0.9} toneMapped={false} />
      </mesh>
      <mesh ref={halo} rotation={[0, 0, 0.4]}>
        <torusGeometry args={[1.92, 0.012, 16, 160]} />
        <meshStandardMaterial
          color="#2dd4bf"
          emissive="#2dd4bf"
          emissiveIntensity={0.6}
          toneMapped={false}
          transparent
          opacity={0.5}
        />
      </mesh>
    </group>
  );
}

export function JarvisRing({ analyser, speaking = false, className }: Props) {
  return (
    <div className={className}>
      <Canvas camera={{ position: [0, 0, 5], fov: 45 }} dpr={[1, 2]} gl={{ alpha: true, antialias: true }}>
        <ambientLight intensity={0.6} />
        <pointLight position={[0, 0, 4]} intensity={2} color="#5eead4" />
        <Core analyser={analyser} speaking={speaking} />
      </Canvas>
    </div>
  );
}
