import * as T from 'three';
import { RoundedBoxGeometry } from 'three/addons/geometries/RoundedBoxGeometry.js';

export interface RoomRenderer { setMode: (mode: string, room: unknown, reduced: boolean) => void; dispose: () => void }

/** One small, demand-rendered context. All furniture is local geometry, with no network assets. */
export function createRoomRenderer(host: HTMLElement, onContextLost: () => void): RoomRenderer {
  const renderer = new T.WebGLRenderer({ alpha: true, antialias: true, powerPreference: 'low-power', preserveDrawingBuffer: false });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.5));
  renderer.outputColorSpace = T.SRGBColorSpace;
  renderer.toneMapping = T.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.25;
  renderer.setClearColor(0x000000, 0);
  renderer.domElement.setAttribute('aria-hidden', 'true');
  host.appendChild(renderer.domElement);
  const stage = host.closest('.sound-visual') ?? host;
  const camera = new T.PerspectiveCamera(38, 2, .1, 40);
  const look = new T.Vector3(0, 1.25, -.7);
  let scene = new T.Scene();
  let currentMode = 'standard';
  let currentRoom: unknown = 0;
  let reduced = false;
  let disposed = false;
  let visible = false;
  let frame = 0;
  let dirty = true;
  let pointerX = 0;
  let pointerY = 0;
  let cameraX = 0;
  let cameraY = 3;
  const geometries = new Set<T.BufferGeometry>();
  const materials = new Set<T.Material>();
  const textures = new Set<T.Texture>();

  function releaseScene() {
    scene.traverse(node => { if (node instanceof T.InstancedMesh) node.dispose(); });
    geometries.forEach(g => g.dispose()); geometries.clear();
    materials.forEach(m => m.dispose()); materials.clear();
    textures.forEach(t => t.dispose()); textures.clear();
    scene.clear();
    renderer.renderLists.dispose();
  }

  function build() {
    releaseScene();
    scene = new T.Scene();
    const light = document.documentElement.dataset.theme === 'light';
    const cinema = currentMode === 'cinema';
    const cafe = currentMode === 'backgroundMusic' && currentRoom === 2;
    const warm = currentMode === 'backgroundMusic' || currentMode === 'music';
    const background = light ? 0xfaf8f1 : 0x191b1e;
    scene.fog = new T.Fog(background, 10, 18);
    scene.add(new T.HemisphereLight(warm ? 0xffe3ba : 0xdde9ff, 0x292521, light ? 2.3 : 1.5));
    const key = new T.DirectionalLight(warm ? 0xffd29b : 0xc4dcff, light ? 3 : 2.1);
    key.position.set(-3, 6, 4); scene.add(key);
    const fill = new T.DirectionalLight(0xffffff, .45); fill.position.set(4, 2, -3); scene.add(fill);
    const cache = new Map<string, T.MeshStandardMaterial>();
    const mat = (color: number, emissive = false, metalness = 0) => {
      const k = `${color}/${emissive}/${metalness}`;
      if (!cache.has(k)) {
        const material = new T.MeshStandardMaterial({ color, roughness: metalness ? .42 : .9, metalness, emissive: emissive ? color : 0x000000, emissiveIntensity: emissive ? .65 : 0 });
        cache.set(k, material); materials.add(material);
      }
      return cache.get(k)!;
    };
    const geo = <G extends T.BufferGeometry>(g: G): G => { geometries.add(g); return g; };
    const unit = geo(new T.BoxGeometry(1, 1, 1));
    const box = (x: number, y: number, z: number, w: number, h: number, d: number, color: number, glow = false) => {
      const mesh = new T.Mesh(unit, mat(color, glow)); mesh.position.set(x, y, z); mesh.scale.set(w, h, d); scene.add(mesh); return mesh;
    };
    const round = (x: number, y: number, z: number, w: number, h: number, d: number, color: number, radius = .06) => {
      const mesh = new T.Mesh(geo(new RoundedBoxGeometry(w, h, d, 2, radius)), mat(color)); mesh.position.set(x, y, z); scene.add(mesh); return mesh;
    };
    const cylinder = (x: number, y: number, z: number, radius: number, height: number, color: number, top = radius) => {
      const mesh = new T.Mesh(geo(new T.CylinderGeometry(top, radius, height, 16)), mat(color)); mesh.position.set(x, y, z); scene.add(mesh); return mesh;
    };
    const orb = (x: number, y: number, z: number, r: number, color: number, glow = false) => {
      const mesh = new T.Mesh(geo(new T.SphereGeometry(r, 12, 8)), mat(color, glow)); mesh.position.set(x, y, z); scene.add(mesh); return mesh;
    };
    const shadowCanvas = document.createElement('canvas'); shadowCanvas.width = shadowCanvas.height = 64;
    const context = shadowCanvas.getContext('2d')!;
    const gradient = context.createRadialGradient(32, 32, 1, 32, 32, 32); gradient.addColorStop(0, 'rgba(0,0,0,.34)'); gradient.addColorStop(1, 'rgba(0,0,0,0)');
    context.fillStyle = gradient; context.fillRect(0, 0, 64, 64);
    const shadowTexture = new T.CanvasTexture(shadowCanvas); textures.add(shadowTexture);
    const shadowMat = new T.MeshBasicMaterial({ map: shadowTexture, transparent: true, depthWrite: false }); materials.add(shadowMat);
    const shadowGeo = geo(new T.PlaneGeometry(1, 1));
    const shadow = (x: number, z: number, w: number, d: number) => {
      const mesh = new T.Mesh(shadowGeo, shadowMat); mesh.rotation.x = -Math.PI / 2; mesh.position.set(x, .017, z); mesh.scale.set(w, d, 1); scene.add(mesh);
    };
    const wood = light ? 0xb18d62 : 0x71523b;
    const wall = light ? 0xded7c6 : warm ? 0x443b31 : 0x252a32;
    box(0, -.08, -.5, 8.4, .16, 8, cinema ? 0x211c24 : warm ? wood : light ? 0xc2bdb1 : 0x2c3035);
    box(0, 2.1, -3.3, 8.4, 4.3, .12, cinema ? 0x151722 : wall);
    box(-4.15, 2.1, -.5, .12, 4.3, 5.8, wall);
    box(4.15, 2.1, -.5, .12, 4.3, 5.8, wall);
    if (warm) for (let i = -10; i <= 10; i++) box(i * .4, .006, -.5, .008, .008, 7.8, light ? 0x967957 : 0x584330);

    const plant = (x: number, z: number, scale = 1) => {
      cylinder(x, .25 * scale, z, .23 * scale, .5 * scale, 0x8c7661, .29 * scale); shadow(x, z, 1, .9);
      for (let i = 0; i < 7; i++) {
        const leaf = orb(x + Math.sin(i * 2.4) * .22 * scale, (.65 + i * .11) * scale, z + Math.cos(i * 2.4) * .16 * scale, .23 * scale, i % 2 ? 0x3e5b42 : 0x5a7450);
        leaf.scale.set(.6, 1.8, .7); leaf.rotation.z = Math.sin(i * 3) * .6;
      }
    };
    const pendant = (x: number, z: number) => {
      cylinder(x, 3.5, z, .012, 1, 0x3b332c);
      cylinder(x, 2.94, z, .34, .2, 0x6c543b, .16);
      orb(x, 2.83, z, .09, 0xffd79b, true);
    };
    const table = (x: number, z: number) => {
      shadow(x, z, 1.7, 1.4); cylinder(x, .48, z, .07, .93, 0x332e28); cylinder(x, .045, z, .36, .06, 0x332e28);
      cylinder(x, .96, z, .61, .09, 0xb18a5b); cylinder(x - .15, 1.07, z, .075, .14, 0xe9dcc7);
    };

    if (cinema) {
      // Screen, proscenium, velvet curtain folds and rows seen from the rear.
      round(0, 2.45, -3.13, 5.7, 2.35, .16, 0x090d17, .08);
      box(0, 2.47, -3.02, 5.38, 2.08, .035, light ? 0x99b6cb : 0x658daa, true);
      box(0, 1.32, -2.9, 6, .1, .25, 0x32262a);
      for (const side of [-1, 1]) {
        for (let i = 0; i < 7; i++) cylinder(side * (3.12 + i * .13), 2.12, -3.02 + (i % 2) * .05, .1, 4.1, i % 2 ? 0x4e2030 : 0x672b38);
        box(side * 3.66, .1, .1, .035, .025, 5.6, 0xd49b58, true);
        for (let i = 0; i < 3; i++) box(side * 3.98, 1.45, -1.6 + i * 1.55, .035, .24, .2, 0xe0b372, true);
      }
      const backs = new T.InstancedMesh(geo(new RoundedBoxGeometry(.5, .6, .16, 2, .07)), mat(0x6b2b3d), 24);
      const seats = new T.InstancedMesh(geo(new RoundedBoxGeometry(.5, .15, .44, 2, .06)), mat(0x542132), 24);
      const feet = new T.InstancedMesh(unit, mat(0x19191f), 24);
      const object = new T.Object3D(); let index = 0;
      for (let row = 0; row < 3; row++) for (const side of [-1, 1]) for (let col = 0; col < 4; col++) {
        const x = side * (1.25 + col * .65), z = -.5 + row * 1.16;
        object.position.set(x, .68 + row * .08, z); object.scale.set(1, 1, 1); object.updateMatrix(); backs.setMatrixAt(index, object.matrix);
        object.position.set(x, .42 + row * .08, z - .16); object.updateMatrix(); seats.setMatrixAt(index, object.matrix);
        object.position.set(x, .21, z - .1); object.scale.set(.1, .42, .1); object.updateMatrix(); feet.setMatrixAt(index++, object.matrix);
      }
      scene.add(backs, seats, feet);
    } else if (cafe) {
      // Warm corner café: tall window, counter, espresso machine, pendants and bistro tables.
      box(-2.15, 2.08, -3.19, 2.55, 2.6, .05, 0x243f43);
      for (const x of [-3.47, -2.15, -.83]) box(x, 2.08, -3.12, .055, 2.7, .06, 0xa18b68);
      box(-2.15, 2.1, -3.1, 2.7, .06, .08, 0xa18b68);
      box(-2.15, .75, -2.95, 2.8, .12, .42, wood);
      round(2.35, .61, -2.35, 2.7, 1.22, .78, 0x3f625c);
      box(2.35, 1.28, -2.35, 2.88, .12, .98, 0xb39b7d);
      box(2.7, 1.62, -2.43, .8, .55, .5, 0x272b2c); box(2.7, 1.72, -2.15, .58, .18, .04, 0x9e9a8d);
      for (let i = 0; i < 4; i++) { cylinder(1.35 + i * .24, 1.45, -2.16, .07, .19, 0xe2d4b9); box(1.5 + i * .45, 2.42, -3.05, .2, .32, .13, i % 2 ? 0x816b49 : 0xc9b89a); }
      box(2.2, 2.2, -3.08, 2.55, .07, .34, wood);
      pendant(-2.15, -.4); pendant(2.15, -.4);
      table(-2.55, 1.15); table(2.6, 1.15);
      for (const x of [-3.3, -1.8, 1.85, 3.35]) {
        cylinder(x, .58, 1.6, .27, .08, wood); cylinder(x, .27, 1.6, .045, .54, 0x302d28); shadow(x, 1.6, .65, .55);
      }
      plant(-3.55, -1.65, .9);
    } else if (currentMode === 'backgroundMusic' && currentRoom === 1) {
      // Living room: sofa, reading light, rug, low table and framed prints.
      box(0, .015, .5, 6.4, .02, 4.2, light ? 0xc5bca7 : 0x60594d);
      round(-1.9, .48, -1.8, 2.7, .58, 1.05, 0x7a8376); round(-1.9, .98, -2.2, 2.7, .95, .28, 0x7a8376);
      for (const x of [-3.17, -.63]) round(x, .72, -1.8, .26, .73, 1.12, 0x697367);
      for (const x of [-2.5, -1.3]) round(x, .83, -1.92, .75, .43, .19, 0xb8a98e);
      shadow(-1.9, -1.8, 3.5, 2); table(1.8, .7); plant(3.2, -1.9, 1.15);
      cylinder(-3.4, 1.15, -.1, .026, 2.3, 0x70604a); cylinder(-3.4, 2.25, -.1, .35, .42, 0xdcc8a8, .22); orb(-3.4, 2.08, -.1, .1, 0xffd8a0, true);
      for (const x of [-2.8, -1.4]) { box(x, 2.65, -3.17, 1.05, 1.35, .08, wood); box(x, 2.65, -3.11, .93, 1.22, .025, x < -2 ? 0xb3ae91 : 0x747f7b); }
    } else if (currentMode === 'backgroundMusic') {
      // Personal room: work desk, monitor, books and a reading lamp.
      box(-2, 1, -1.7, 2.75, .1, 1.1, wood);
      for (const x of [-3.2, -.8]) for (const z of [-2.1, -1.3]) box(x, .5, z, .07, 1, .07, 0x47443e);
      box(-2.2, 1.58, -1.9, 1.1, .75, .09, 0x202c31); box(-2.2, 1.18, -1.9, .07, .3, .1, 0x505253);
      cylinder(-3.1, 1.4, -1.55, .025, .7, 0x8f7751); orb(-3.1, 1.78, -1.55, .12, 0xffdba1, true);
      for (const x of [2.2, 3.55]) box(x, 1.45, -2.4, .08, 2.9, .6, wood);
      for (let row = 0; row < 4; row++) {
        box(2.86, .25 + row * .72, -2.4, 1.42, .065, .65, wood);
        for (let i = 0; i < 6; i++) box(2.38 + i * .18, .48 + row * .72, -2.42, .12, .4 + (i % 2) * .08, .35, [0x8d8069, 0x4b6867, 0x89624e][i % 3]);
      }
      plant(3.25, .4, .85); shadow(-2, -1.7, 3, 1.6);
      round(-1.7, .62, .05, .74, .16, .7, 0x655e54); round(-1.7, 1.03, .32, .74, .78, .14, 0x655e54);
    } else if (currentMode === 'music') {
      box(0, .12, -1.4, 7, .24, 3.2, 0x43392f);
      for (const x of [-3, 3]) { box(x, 1.15, -2.1, .78, 2.1, .65, 0x202225); for (const y of [.7, 1.55]) { const cone = cylinder(x, y, -1.74, .25, .04, 0x44464a); cone.rotation.x = Math.PI / 2; } }
      for (let i = -12; i <= 12; i++) box(i * .29, 2.1, -3.13, .09, 3.7, .2, i % 2 ? 0x746044 : 0x9a7c51);
      pendant(-2, -.4); pendant(2, -.4);
    } else if (currentMode === 'game') {
      for (const side of [-1, 1]) {
        for (let i = 0; i < 4; i++) { const panel = box(side * (2.2 + i * .48), 1.9, -2.5 + i * .55, .22, 3, .7, 0x26383c); panel.rotation.y = side * -.25; }
        box(side * 3.15, .025, .3, .025, .02, 5.5, 0x70b6b2, true);
        box(side * 2.2, 1.8, -2.1, .025, 2.3, .06, 0x70b6b2, true);
      }
    } else {
      for (const side of [-1, 1]) for (let i = 0; i < 4; i++) box(side * (2.5 + i * .32), 2, -2.9, .055, 3.3, .08, light ? 0xc9cbc4 : 0x30343a);
    }
  }

  function draw() {
    frame = 0;
    if (disposed || !visible || document.hidden) return;
    const bounds = host.getBoundingClientRect();
    if (!bounds.width || !bounds.height) return;
    const width = Math.min(1000, Math.round(bounds.width)), height = Math.max(1, Math.round(width * bounds.height / bounds.width));
    if (renderer.domElement.width !== Math.round(width * renderer.getPixelRatio()) || renderer.domElement.height !== Math.round(height * renderer.getPixelRatio())) renderer.setSize(width, height, false);
    camera.aspect = bounds.width / bounds.height; camera.updateProjectionMatrix();
    const targetX = reduced ? 0 : pointerX * .16, targetY = 3.05 + (reduced ? 0 : pointerY * .08);
    cameraX += (targetX - cameraX) * .18; cameraY += (targetY - cameraY) * .18;
    camera.position.set(cameraX, cameraY, camera.aspect < 1.5 ? 9.8 : 8.2); camera.lookAt(look);
    renderer.render(scene, camera); dirty = false;
    if (Math.abs(targetX - cameraX) + Math.abs(targetY - cameraY) > .002) requestDraw();
  }
  function requestDraw() { dirty = true; if (!frame && !disposed && visible && !document.hidden) frame = requestAnimationFrame(draw); }
  function move(event: Event) {
    if (reduced) return;
    const e = event as PointerEvent; const rect = stage.getBoundingClientRect();
    pointerX = (e.clientX - rect.left) / rect.width * 2 - 1; pointerY = (e.clientY - rect.top) / rect.height * 2 - 1; requestDraw();
  }
  function leave() { pointerX = pointerY = 0; requestDraw(); }
  function sync() { if (!visible || document.hidden) { cancelAnimationFrame(frame); frame = 0; } else if (dirty) requestDraw(); }
  const observer = new IntersectionObserver(([entry]) => { visible = entry.isIntersecting; sync(); }); observer.observe(host);
  const resize = new ResizeObserver(requestDraw); resize.observe(host);
  const theme = new MutationObserver(() => { build(); requestDraw(); }); theme.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
  const lost = (event: Event) => { event.preventDefault(); cancelAnimationFrame(frame); frame = 0; onContextLost(); };
  renderer.domElement.addEventListener('webglcontextlost', lost);
  stage.addEventListener('pointermove', move); stage.addEventListener('pointerleave', leave); document.addEventListener('visibilitychange', sync);
  return {
    setMode(mode, room, reduce) { currentMode = mode; currentRoom = room; reduced = reduce; pointerX = pointerY = 0; cameraX = 0; cameraY = reduced ? 3.05 : 3.15; build(); requestDraw(); },
    dispose() {
      disposed = true; cancelAnimationFrame(frame); observer.disconnect(); resize.disconnect(); theme.disconnect();
      stage.removeEventListener('pointermove', move); stage.removeEventListener('pointerleave', leave); document.removeEventListener('visibilitychange', sync);
      renderer.domElement.removeEventListener('webglcontextlost', lost); releaseScene(); renderer.dispose(); renderer.forceContextLoss(); renderer.domElement.remove();
    },
  };
}
