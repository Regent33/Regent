'use client';
// The kinetic particle core — a tilted teal particle ring (Three.js) that
// breathes when idle and swells with speech amplitude (mic while listening,
// reply audio while speaking; both feed the same analyser). GSAP tweens the
// per-phase energy/speed so state changes glide instead of snapping.
// Reduced motion: the ring renders once, static.
import { useEffect, useRef } from 'react';
import * as THREE from 'three';
import gsap from 'gsap';
import type { CallPhase } from '@/features/butler/domain/phase';

const COUNT = 2600;
const RING_SHARE = 0.8; // rest scatter inside for depth

const PHASE_PARAMS: Record<CallPhase, { energy: number; speed: number }> = {
  connecting: { energy: 0.12, speed: 0.04 },
  listening: { energy: 0.5, speed: 0.1 },
  thinking: { energy: 0.35, speed: 0.5 },
  speaking: { energy: 0.85, speed: 0.22 },
};

export function ParticleCore({
  phase,
  analyserRef,
}: {
  phase: CallPhase;
  analyserRef: React.RefObject<AnalyserNode | null>;
}) {
  const mountRef = useRef<HTMLDivElement>(null);
  const paramsRef = useRef({ ...PHASE_PARAMS.connecting });

  useEffect(() => {
    const target = PHASE_PARAMS[phase];
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
      Object.assign(paramsRef.current, target);
      return;
    }
    const tween = gsap.to(paramsRef.current, { ...target, duration: 0.6, ease: 'power2.out' });
    return () => {
      tween.kill();
    };
  }, [phase]);

  useEffect(() => {
    const mount = mountRef.current;
    if (!mount) return;
    const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;

    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
    mount.appendChild(renderer.domElement);
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(45, 1, 0.1, 10);
    camera.position.z = 3.2;

    // Accent from the token layer — the core recolors with the theme.
    const accent = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim();

    const positions = new Float32Array(COUNT * 3);
    for (let i = 0; i < COUNT; i++) {
      const angle = Math.random() * Math.PI * 2;
      const onRing = i < COUNT * RING_SHARE;
      // Ring points hug radius 1 with slight gaussian-ish jitter; the rest
      // scatter inside as a sparse core.
      const r = onRing
        ? 1 + (Math.random() + Math.random() - 1) * 0.06
        : Math.sqrt(Math.random()) * 0.85;
      positions[i * 3] = Math.cos(angle) * r;
      positions[i * 3 + 1] = Math.sin(angle) * r;
      positions[i * 3 + 2] = (Math.random() - 0.5) * (onRing ? 0.05 : 0.3);
    }
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    const material = new THREE.PointsMaterial({
      color: new THREE.Color(accent),
      size: 0.014,
      transparent: true,
      opacity: 0.8,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    const points = new THREE.Points(geometry, material);
    points.rotation.x = 0.55; // tilt for depth, like the JARVIS refs
    scene.add(points);

    const freq = new Uint8Array(128);
    const amplitude = (): number => {
      const analyser = analyserRef.current;
      if (!analyser) return 0;
      analyser.getByteFrequencyData(freq);
      let sum = 0;
      for (const v of freq) sum += v;
      return sum / freq.length / 255;
    };

    const resize = () => {
      const s = Math.min(mount.clientWidth, mount.clientHeight);
      renderer.setSize(s, s);
    };
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(mount);

    let raf = 0;
    let angle = 0;
    let last = performance.now();
    const frame = (now: number) => {
      const dt = (now - last) / 1000;
      last = now;
      const { energy, speed } = paramsRef.current;
      const amp = amplitude();
      angle += dt * speed;
      points.rotation.z = angle;
      const breathe = Math.sin(now / 900) * 0.02;
      points.scale.setScalar(1 + breathe + amp * 0.35 * (0.5 + energy));
      material.opacity = Math.min(1, 0.45 + energy * 0.4 + amp * 0.25);
      material.size = 0.012 + energy * 0.006 + amp * 0.012;
      renderer.render(scene, camera);
      raf = requestAnimationFrame(frame);
    };
    if (reduced) {
      renderer.render(scene, camera); // one static frame
    } else {
      raf = requestAnimationFrame(frame);
    }

    return () => {
      cancelAnimationFrame(raf);
      observer.disconnect();
      geometry.dispose();
      material.dispose();
      renderer.dispose();
      mount.removeChild(renderer.domElement);
    };
  }, [analyserRef]);

  return <div ref={mountRef} className="size-full" aria-hidden />;
}
