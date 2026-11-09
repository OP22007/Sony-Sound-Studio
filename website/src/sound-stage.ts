import * as THREE from 'three';

export type SoundStageMode = 'standard' | 'cinema' | 'ambient';

export interface SoundStageController {
  setMode(mode: SoundStageMode): void;
  dispose(): void;
}

/** Mounts a transparent, restrained acoustic sculpture behind the HTML product image. */
export async function mountSoundStage(
  container: HTMLElement,
  reducedMotion: MediaQueryList,
): Promise<SoundStageController> {
  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(35, 1, 0.1, 80);
  camera.position.set(0, 0, 12.8);

  const renderer = new THREE.WebGLRenderer({
    alpha: true,
    antialias: true,
    powerPreference: 'low-power',
    premultipliedAlpha: false,
  });
  renderer.setClearColor(0x000000, 0);
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, window.innerWidth <= 800 ? 1 : 1.5));
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.domElement.setAttribute('aria-hidden', 'true');
  Object.assign(renderer.domElement.style, {
    position: 'absolute', inset: '0', width: '100%', height: '100%',
    display: 'block', pointerEvents: 'none',
  });
  container.appendChild(renderer.domElement);

  const sculpture = new THREE.Group();
  sculpture.rotation.x = -0.13;
  sculpture.rotation.y = -0.08;
  scene.add(sculpture);
  const field = new THREE.Group();
  sculpture.add(field);

  const resources: Array<{ dispose(): void }> = [];
  const ringMaterials: THREE.LineBasicMaterial[] = [];
  const ringBaseOpacity: number[] = [];
  const makeRing = (rx: number, ry: number, z: number, color: number, opacity: number, phase = 0) => {
    const points: THREE.Vector3[] = [];
    const samples = 220;
    for (let i = 0; i <= samples; i++) {
      const t = i / samples * Math.PI * 2;
      const wave = 1 + 0.003 * Math.sin(t * 6 + phase) + 0.002 * Math.cos(t * 11 - phase);
      points.push(new THREE.Vector3(Math.cos(t) * rx * wave, Math.sin(t) * ry * wave, z));
    }
    const geometry = new THREE.BufferGeometry().setFromPoints(points);
    const material = new THREE.LineBasicMaterial({ color, transparent: true, opacity, depthWrite: false });
    const line = new THREE.Line(geometry, material);
    field.add(line);
    resources.push(geometry, material);
    ringMaterials.push(material);
    ringBaseOpacity.push(opacity);
    return line;
  };

  // Broad contour rails enclose the photo area, with a slight depth drift like a machined diaphragm.
  for (let i = 0; i < 11; i++) {
    const t = i / 10;
    makeRing(2.42 + t * 1.55, 1.58 + t * 1.13, -0.86 + t * 0.09,
      i === 5 ? 0xb68a4e : 0x575650, i === 5 ? 0.48 : 0.15 + (i % 3) * 0.025, i * 0.42);
  }
  // Fine inset contour, intentionally thin and warm, not a luminous halo.
  makeRing(2.19, 1.42, -0.36, 0xc29a62, 0.25, 1.1);
  makeRing(4.12, 2.94, 0.24, 0x77746c, 0.12, 2.4);

  const finMaterial = new THREE.MeshBasicMaterial({ color: 0x7b766b, transparent: true, opacity: 0.18, depthWrite: false });
  resources.push(finMaterial);
  const finGeo = new THREE.BoxGeometry(0.018, 0.54, 0.045);
  resources.push(finGeo);
  const fins = new THREE.Group();
  field.add(fins);
  // Short vertical ribs live around the perimeter, leaving generous negative space for the product.
  for (let i = 0; i < 30; i++) {
    const a = (i / 30) * Math.PI * 2;
    const x = Math.cos(a) * 3.35;
    const y = Math.sin(a) * 2.5;
    if (Math.abs(x) < 2.15 && Math.abs(y) < 1.42) continue;
    const mesh = new THREE.Mesh(finGeo, finMaterial);
    mesh.position.set(x, y, -0.52 + Math.sin(a * 2) * 0.07);
    mesh.rotation.z = a + Math.PI / 2;
    mesh.scale.y = 0.55 + 0.45 * (0.5 + 0.5 * Math.sin(i * 2.37));
    fins.add(mesh);
  }

  // A few etched points add scale and parallax without turning the frame into a particle field.
  const particlePositions = new Float32Array(36 * 3);
  for (let i = 0; i < 36; i++) {
    const angle = i * 2.399963;
    const radius = 2.55 + (i % 9) * 0.19;
    particlePositions[i * 3] = Math.cos(angle) * radius;
    particlePositions[i * 3 + 1] = Math.sin(angle) * radius * 0.73;
    particlePositions[i * 3 + 2] = -0.7 + (i % 4) * 0.2;
  }
  const particleGeometry = new THREE.BufferGeometry();
  particleGeometry.setAttribute('position', new THREE.BufferAttribute(particlePositions, 3));
  const particleMaterial = new THREE.PointsMaterial({ color: 0x977b55, size: 0.022, transparent: true, opacity: 0.38, sizeAttenuation: true, depthWrite: false });
  const particles = new THREE.Points(particleGeometry, particleMaterial);
  field.add(particles);
  resources.push(particleGeometry, particleMaterial);

  const state = {
    mode: 'standard' as SoundStageMode,
    targetX: 0, targetY: 0,
    targetScaleX: 1, targetScaleY: 1,
    pointerX: 0, pointerY: 0,
    scrollTilt: 0,
    visible: false,
    contextLost: false,
    disposed: false,
    raf: 0,
    lastTime: 0,
  };

  const resize = () => {
    const width = Math.max(1, container.clientWidth);
    const height = Math.max(1, container.clientHeight);
    renderer.setSize(width, height, false);
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
  };
  resize();
  const resizeObserver = new ResizeObserver(() => { resize(); wake(); });
  resizeObserver.observe(container);

  const render = () => renderer.render(scene, camera);
  const animate = (time: number) => {
    state.raf = 0;
    if (state.disposed || state.contextLost || !state.visible || document.visibilityState === 'hidden') return;
    const dt = state.lastTime ? Math.min((time - state.lastTime) / 1000, 0.05) : 0.016;
    state.lastTime = time;
    const follow = 1 - Math.exp(-dt * 3.5);
    sculpture.rotation.x += (state.targetX + state.pointerY * 0.035 + state.scrollTilt - sculpture.rotation.x) * follow;
    sculpture.rotation.y += (state.targetY + state.pointerX * 0.045 - sculpture.rotation.y) * follow;
    field.scale.x += (state.targetScaleX - field.scale.x) * follow;
    field.scale.y += (state.targetScaleY - field.scale.y) * follow;
    const timeFactor = state.mode === 'ambient' ? 0.22 : state.mode === 'cinema' ? 0.65 : 0.42;
    sculpture.rotation.z = Math.sin(time * 0.00013) * timeFactor * 0.008;
    render();
    if (!reducedMotion.matches) state.raf = requestAnimationFrame(animate);
  };
  const wake = () => {
    if (state.disposed || state.contextLost || !state.visible || document.visibilityState === 'hidden') return;
    if (reducedMotion.matches) {
      sculpture.rotation.x = state.targetX + state.pointerY * 0.035 + state.scrollTilt;
      sculpture.rotation.y = state.targetY + state.pointerX * 0.045;
      field.scale.set(state.targetScaleX, state.targetScaleY, 1);
      render();
      return;
    }
    if (!state.raf) state.raf = requestAnimationFrame(animate);
  };
  const visibilityObserver = new IntersectionObserver((entries) => {
    state.visible = entries.some(entry => entry.isIntersecting);
    if (state.visible) wake();
    else if (state.raf) { cancelAnimationFrame(state.raf); state.raf = 0; }
  }, { threshold: 0.01 });
  visibilityObserver.observe(container);

  const interactionTarget = (container.closest('.hero-visual') as HTMLElement | null) ?? container.parentElement ?? container;
  const onPointer = (event: PointerEvent) => {
    const bounds = container.getBoundingClientRect();
    state.pointerX = ((event.clientX - bounds.left) / Math.max(1, bounds.width) - 0.5) * 2;
    state.pointerY = ((event.clientY - bounds.top) / Math.max(1, bounds.height) - 0.5) * 2;
    wake();
  };
  const onLeave = () => { state.pointerX = 0; state.pointerY = 0; wake(); };
  const onScroll = () => {
    const rect = container.getBoundingClientRect();
    state.scrollTilt = THREE.MathUtils.clamp((rect.top + rect.height * 0.5 - innerHeight * 0.5) / innerHeight, -1, 1) * 0.025;
    wake();
  };
  const onDocumentVisibility = () => {
    if (document.visibilityState === 'hidden' && state.raf) { cancelAnimationFrame(state.raf); state.raf = 0; }
    else wake();
  };
  const onMotionChange = () => {
    if (state.raf) { cancelAnimationFrame(state.raf); state.raf = 0; }
    wake();
  };
  const onContextLost = (event: Event) => {
    event.preventDefault();
    state.contextLost = true;
    state.visible = false;
    if (state.raf) { cancelAnimationFrame(state.raf); state.raf = 0; }
    interactionTarget.classList.remove('scene-ready');
    const status = interactionTarget.querySelector<HTMLElement>('.scene-status');
    if (status) status.textContent = 'Static acoustic illustration';
  };
  interactionTarget.addEventListener('pointermove', onPointer, { passive: true });
  interactionTarget.addEventListener('pointerleave', onLeave, { passive: true });
  renderer.domElement.addEventListener('webglcontextlost', onContextLost, false);
  window.addEventListener('scroll', onScroll, { passive: true });
  document.addEventListener('visibilitychange', onDocumentVisibility);
  reducedMotion.addEventListener('change', onMotionChange);

  const controller: SoundStageController = {
    setMode(mode) {
      state.mode = mode;
      const target = mode === 'cinema' ? 0.045 : mode === 'ambient' ? -0.055 : 0;
      state.targetX = -0.13 + target;
      state.targetY = -0.08 + (mode === 'cinema' ? 0.07 : mode === 'ambient' ? -0.04 : 0);
      state.targetScaleX = mode === 'cinema' ? 1.18 : mode === 'ambient' ? 0.93 : 1;
      state.targetScaleY = mode === 'cinema' ? 0.88 : mode === 'ambient' ? 1.12 : 1;
      ringMaterials.forEach((material, index) => {
        const base = ringBaseOpacity[index];
        material.opacity = mode === 'cinema' ? Math.min(0.58, base * 1.18) : mode === 'ambient' ? base * 0.74 : base;
      });
      finMaterial.opacity = mode === 'cinema' ? 0.23 : mode === 'ambient' ? 0.12 : 0.18;
      particleMaterial.opacity = mode === 'ambient' ? 0.2 : mode === 'cinema' ? 0.42 : 0.32;
      wake();
    },
    dispose() {
      if (state.disposed) return;
      state.disposed = true;
      if (state.raf) cancelAnimationFrame(state.raf);
      resizeObserver.disconnect();
      visibilityObserver.disconnect();
      interactionTarget.removeEventListener('pointermove', onPointer);
      interactionTarget.removeEventListener('pointerleave', onLeave);
      renderer.domElement.removeEventListener('webglcontextlost', onContextLost);
      window.removeEventListener('scroll', onScroll);
      document.removeEventListener('visibilitychange', onDocumentVisibility);
      reducedMotion.removeEventListener('change', onMotionChange);
      for (const resource of resources) resource.dispose();
      renderer.dispose();
      renderer.domElement.remove();
    },
  };
  return controller;
}
