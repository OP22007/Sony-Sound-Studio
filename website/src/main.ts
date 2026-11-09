import { configureDownloads } from './platform-downloads';
import { animate, createTimeline, stagger } from 'animejs';
import { mountScrollMotion } from './scroll-motion';
import type { SoundStageController, SoundStageMode } from './sound-stage';

const $ = <T extends Element = HTMLElement>(selector: string) => document.querySelector<T>(selector)!;
const $$ = <T extends Element = HTMLElement>(selector: string) => [...document.querySelectorAll<T>(selector)];
configureDownloads();

const motionPreference = matchMedia('(prefers-reduced-motion: reduce)');
const forceStatic = matchMedia('(min-width: 0px)');
let motionPaused = motionPreference.matches;
let scene: SoundStageController | undefined;
let sceneGeneration = 0;
let currentMode: SoundStageMode = 'standard';
let activeCurveAnimation: ReturnType<typeof animate> | undefined;
let activeProductAnimation: ReturnType<typeof animate> | undefined;
let entrance: ReturnType<typeof createTimeline> | undefined;
const activeReveals = new Set<ReturnType<typeof animate>>();
const scrollMotion = mountScrollMotion(() => motionPaused);

const menuToggle = $<HTMLButtonElement>('.menu-toggle');
const navigation = $('#navigation');
function closeMenu() {
  navigation.classList.remove('is-open');
  menuToggle.setAttribute('aria-expanded', 'false');
  menuToggle.setAttribute('aria-label', 'Open navigation');
}
menuToggle.addEventListener('click', () => {
  const open = menuToggle.getAttribute('aria-expanded') !== 'true';
  menuToggle.setAttribute('aria-expanded', String(open));
  menuToggle.setAttribute('aria-label', open ? 'Close navigation' : 'Open navigation');
  navigation.classList.toggle('is-open', open);
});
navigation.addEventListener('click', event => { if ((event.target as Element).closest('a')) closeMenu(); });
document.addEventListener('keydown', event => {
  if (event.key === 'Escape' && menuToggle.getAttribute('aria-expanded') === 'true') { closeMenu(); menuToggle.focus(); }
});
matchMedia('(min-width: 581px)').addEventListener('change', closeMenu);

if (!motionPaused) {
  entrance = createTimeline({ defaults: { ease: 'out(4)', duration: 950 } })
    .add('.line > span', { y: ['105%', '0%'], opacity: [0, 1], delay: stagger(110) }, 120)
    .add('.hero-entrance', { y: [17, 0], opacity: [0, 1], delay: stagger(90) }, 320)
    .add('.product-float', { y: [40, 0], rotate: [3, 0], opacity: [0, 1], duration: 1500 }, 190)
    .add('.hero-bottom, .model-annotation', { y: [10, 0], opacity: [0, 1], delay: stagger(90) }, 800);
}

const revealObserver = new IntersectionObserver(entries => {
  for (const entry of entries) {
    if (!entry.isIntersecting) continue;
    revealObserver.unobserve(entry.target);
    if (!motionPaused) {
      const animation = animate(entry.target, {
        y: [25, 0], opacity: [.1, 1], duration: 800, ease: 'out(4)',
        onComplete: () => activeReveals.delete(animation),
      });
      activeReveals.add(animation);
    }
  }
}, { threshold: .09 });
$$('[data-reveal]').forEach(element => revealObserver.observe(element));

const captions: Record<SoundStageMode, string> = {
  standard: 'Standard · A focused, balanced sound field. Visual demonstration; no device settings are changed.',
  cinema: 'Cinema · A wider, deeper stage. Native Cinema requires supported headphones; this is a visual demonstration.',
  ambient: 'Ambient · A more open space. Visual demonstration; your headphones and surroundings are unchanged.',
};
$$<HTMLButtonElement>('.hero-mode-controls [data-mode]').forEach(button => {
  button.addEventListener('click', () => {
    currentMode = button.dataset.mode as SoundStageMode;
    $$('.hero-mode-controls [data-mode]').forEach(item => item.setAttribute('aria-pressed', String(item === button)));
    $('.hero-visual').setAttribute('data-mode', currentMode);
    $('#mode-caption').textContent = captions[currentMode];
    scene?.setMode(currentMode);
    activeProductAnimation?.pause();
    if (!motionPaused) activeProductAnimation = animate('.product-float', {
      rotate: currentMode === 'cinema' ? -2 : currentMode === 'ambient' ? 2 : 0,
      y: currentMode === 'cinema' ? -9 : currentMode === 'ambient' ? -3 : 0,
      duration: 1000, ease: 'out(4)',
    });
  });
});

async function mountScene() {
  const generation = ++sceneGeneration;
  scene?.dispose();
  scene = undefined;
  try {
    const { mountSoundStage } = await import('./sound-stage');
    if (generation !== sceneGeneration) return;
    const controller = await mountSoundStage($('#sound-stage'), motionPaused ? forceStatic : motionPreference);
    if (generation !== sceneGeneration) { controller.dispose(); return; }
    scene = controller;
    scene.setMode(currentMode);
    $('.hero-visual').classList.add('scene-ready');
    $('#scene-status').textContent = motionPaused ? '3D SOUND FIELD / STILL' : '3D SOUND FIELD / MOVE TO EXPLORE';
  } catch (error) {
    if (import.meta.env.DEV) console.warn('Sound field initialization failed:', error);
    if (generation !== sceneGeneration) return;
    $('.hero-visual').classList.remove('scene-ready');
    $('#scene-status').textContent = 'PRODUCT PHOTOGRAPHY / STATIC VIEW';
    $('#sound-stage').replaceChildren();
  }
}
const sceneObserver = new IntersectionObserver(entries => {
  if (entries.some(entry => entry.isIntersecting)) { sceneObserver.disconnect(); void mountScene(); }
}, { rootMargin: '100px' });
sceneObserver.observe($('.hero-visual'));

function updateMotion() {
  if (motionPreference.matches) motionPaused = true;
  document.documentElement.dataset.motion = motionPaused ? 'paused' : 'running';
  scrollMotion.refresh();
  $('#motion-toggle').setAttribute('aria-pressed', String(motionPaused));
  $<HTMLButtonElement>('#motion-toggle').disabled = motionPreference.matches;
  $('#motion-toggle').textContent = motionPreference.matches ? 'Motion reduced by system' : motionPaused ? 'Resume motion' : 'Pause motion';
  if (motionPaused) {
    entrance?.complete();
    activeCurveAnimation?.complete();
    activeProductAnimation?.complete();
    for (const animation of activeReveals) animation.complete();
    activeReveals.clear();
  }
  if (scene) void mountScene();
}
$('#motion-toggle').addEventListener('click', () => { if (!motionPreference.matches) { motionPaused = !motionPaused; updateMotion(); } });
motionPreference.addEventListener('change', () => { motionPaused = motionPreference.matches; updateMotion(); });
updateMotion();

const presets: Record<string, number[]> = {
  balanced: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  warm: [3, 3.6, 2.8, 1.1, -.5, -1, -.8, .2, 1, 1.7],
  vocal: [-1.5, -1, 0, 1.5, 3.4, 4.1, 2.8, 1, 0, -.7],
  energetic: [4, 3.4, 1, -.6, -1.7, -1.2, .8, 2.8, 3.6, 2.5],
};
const curveValues: Record<string, number> = Object.fromEntries(Array.from({ length: 10 }, (_, i) => [`v${i}`, 0]));
let currentPreset = 'balanced';
const bassInput = $<HTMLInputElement>('#bass-preview');
const nodes = $('#curve-nodes');
for (let i = 0; i < 10; i++) {
  const circle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
  circle.setAttribute('r', '4.5');
  nodes.appendChild(circle);
}
function drawCurve() {
  const points = Array.from({ length: 10 }, (_, i) => ({ x: 20 + i * 560 / 9, y: 110 - curveValues[`v${i}`] * 12.5 }));
  let path = `M${points[0].x} ${points[0].y}`;
  for (let i = 0; i < points.length - 1; i++) {
    const a = points[Math.max(0, i - 1)], b = points[i], c = points[i + 1], d = points[Math.min(points.length - 1, i + 2)];
    path += ` C${b.x + (c.x - a.x) / 6} ${b.y + (c.y - a.y) / 6},${c.x - (d.x - b.x) / 6} ${c.y - (d.y - b.y) / 6},${c.x} ${c.y}`;
  }
  $('#curve').setAttribute('d', path);
  $('#curve-fill').setAttribute('d', `${path}L580 200H20Z`);
  [...nodes.children].forEach((node, i) => { node.setAttribute('cx', String(points[i].x)); node.setAttribute('cy', String(points[i].y)); });
}
function updateCurve() {
  activeCurveAnimation?.pause();
  const bass = Number(bassInput.value);
  $('#bass-value').textContent = bass > 0 ? `+${bass}` : String(bass);
  const target = Object.fromEntries(presets[currentPreset].map((value, i) => [`v${i}`, Math.min(6, Math.max(-6, value + bass * ([1, .65, .2][i] ?? 0)))]));
  if (motionPaused) { Object.assign(curveValues, target); drawCurve(); }
  else activeCurveAnimation = animate(curveValues, { ...target, duration: 450, ease: 'out(4)', onUpdate: drawCurve });
}
$$<HTMLButtonElement>('[data-preset]').forEach(button => button.addEventListener('click', () => {
  currentPreset = button.dataset.preset!;
  $$('[data-preset]').forEach(item => item.setAttribute('aria-pressed', String(item === button)));
  updateCurve();
}));
bassInput.addEventListener('input', updateCurve);
drawCurve();

const models: Record<string, { name: string; image: string; status: string; description: string; capabilities: string[]; caveat: string; index: string }> = {
  xm6: { name: 'WH–1000XM6', image: 'wh-1000xm6-black.png', status: 'HARDWARE-TESTED BASELINE', description: 'The starting point. Noise-mode tracking and an EQ-band change/readback tested on Windows with firmware 3.1.5.', capabilities: ['Noise control', 'Equalizer', 'Listening modes*'], caveat: '* Controls require live capability and device confirmation. Testing does not establish every feature or firmware.', index: '001' },
  xm5: { name: 'WH–1000XM5', image: 'wh-1000xm5-black.png', status: 'HARDWARE VALIDATION NEEDED', description: 'A familiar favorite in our Sony lineup. Discovery and a model-specific studio preview are available; real-device feature coverage still needs testing.', capabilities: ['Device discovery', 'Studio preview'], caveat: 'Do not assume XM6 feature support. Device settings depend on confirmed capabilities, firmware and platform.', index: '002' },
  ch720: { name: 'WH–CH720N', image: 'wh-ch720n-black.png', status: 'HARDWARE VALIDATION NEEDED', description: 'Light on your head, welcome on your desktop. A dedicated model preview and discovery path, with hardware validation still ahead.', capabilities: ['Device discovery', 'Studio preview'], caveat: 'Model-specific control coverage remains unverified. Controls appear only when supported and confirmed.', index: '003' },
  nord: { name: 'Nord Buds 3 Pro', image: 'oneplus-nord-buds-3-pro-mint.png', status: 'EXPERIMENTAL · SOURCE BUILDS', description: 'OnePlus joins the studio. Model-specific ANC profiles, EQ and BassWave are implemented behind live capability checks in source builds.', capabilities: ['Model-specific EQ*', 'ANC / Transparency*', 'BassWave*'], caveat: '* Experimental and awaiting hardware validation. Not included in release 0.1.3. More models are listed in the research catalog.', index: '004' },
};
let modelGeneration = 0;
async function selectModel(key: string) {
  const model = models[key];
  if (!model) return;
  const generation = ++modelGeneration;
  $$('[data-model]').forEach(item => item.setAttribute('aria-pressed', String((item as HTMLElement).dataset.model === key)));
  $('#model-name').textContent = model.name;
  $('#model-description').textContent = model.description;
  $('#model-caveat').textContent = model.caveat;
  $('#model-index').textContent = model.index;
  const status = $('#model-status');
  const dot = document.createElement('i');
  status.replaceChildren(dot, document.createTextNode(` ${model.status}`));
  $('#model-capabilities').replaceChildren(...model.capabilities.map(label => { const span = document.createElement('span'); span.textContent = label; return span; }));
  const image = $<HTMLImageElement>('#model-image');
  image.dataset.busy = 'true';
  const url = `${import.meta.env.BASE_URL}images/${model.image}`;
  const preload = new Image();
  preload.src = url;
  try {
    await preload.decode();
    if (generation !== modelGeneration) return;
    image.src = url;
    image.alt = `${model.name} official product render`;
    image.dataset.busy = 'false';
  } catch {
    if (generation !== modelGeneration) return;
    image.dataset.busy = 'false';
    image.alt = `${model.name} product artwork could not load`;
    image.src = url;
  }
}
$$<HTMLButtonElement>('[data-model]').forEach(button => button.addEventListener('click', () => void selectModel(button.dataset.model!)));
$$<HTMLButtonElement>('[data-brand]').forEach(button => button.addEventListener('click', () => {
  const oneplus = button.dataset.brand === 'oneplus';
  $$('[data-brand]').forEach(item => item.setAttribute('aria-pressed', String(item === button)));
  $$<HTMLButtonElement>('[data-model]').forEach(item => { item.hidden = (item.dataset.model === 'nord') !== oneplus; });
  void selectModel(oneplus ? 'nord' : 'xm6');
}));

window.addEventListener('pagehide', event => {
  if (!event.persisted) scrollMotion.dispose();
  sceneGeneration++;
  scene?.dispose();
  scene = undefined;
  entrance?.pause();
  activeCurveAnimation?.pause();
  activeProductAnimation?.pause();
  activeReveals.forEach(animation => animation.complete());
});
window.addEventListener('pageshow', event => { if (event.persisted) void mountScene(); });
