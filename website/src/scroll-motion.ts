import { animate, stagger } from 'animejs';

/** Native scrolling drives a single, event-scheduled render pass. No scroll interception. */
export function mountScrollMotion(isPaused: () => boolean) {
  const root = document.documentElement;
  const hero = document.querySelector<HTMLElement>('.hero')!;
  const overture = document.querySelector<HTMLElement>('.scroll-overture')!;
  const studio = document.querySelector<HTMLElement>('.studio-window')!;
  const studioShell = document.createElement('div');
  studioShell.className = 'scroll-studio-shell';
  studio.before(studioShell);
  studioShell.append(studio);
  const tuning = document.querySelector<HTMLElement>('.tuning')!;
  const model = document.querySelector<HTMLElement>('.model-art')!;
  const active = new Set<ReturnType<typeof animate>>();
  let frame = 0;
  let disposed = false;
  const clamp = (n: number) => Math.max(0, Math.min(1, n));
  const progress = (rect: DOMRect, vh: number) => clamp((vh - rect.top) / (vh + rect.height));

  // Preserve semantic headings and their existing emphasis/line breaks.
  const headings = [...document.querySelectorAll<HTMLElement>('main h2')];
  for (const heading of headings) {
    const walker = document.createTreeWalker(heading, NodeFilter.SHOW_TEXT);
    const nodes: Text[] = [];
    while (walker.nextNode()) nodes.push(walker.currentNode as Text);
    for (const node of nodes) {
      const fragment = document.createDocumentFragment();
      for (const part of node.data.split(/(\s+)/)) {
        if (!part.trim()) { fragment.append(document.createTextNode(part)); continue; }
        const mask = document.createElement('span');
        mask.className = 'reveal-word-mask';
        const word = document.createElement('span');
        word.className = 'reveal-word';
        word.textContent = part;
        mask.append(word);
        fragment.append(mask);
      }
      node.replaceWith(fragment);
    }
  }
  const headingObserver = new IntersectionObserver(entries => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      headingObserver.unobserve(entry.target);
      if (isPaused()) continue;
      const animation = animate(entry.target.querySelectorAll('.reveal-word'), {
        y: ['110%', '0%'], rotate: [6, 0], opacity: [.25, 1],
        delay: stagger(42), duration: 850, ease: 'out(4)',
        onComplete: () => active.delete(animation),
      });
      active.add(animation);
    }
  }, { threshold: .2 });
  headings.forEach(heading => headingObserver.observe(heading));

  function render() {
    frame = 0;
    if (disposed || document.hidden) return;
    const paused = isPaused();
    root.dataset.scrollMotion = paused ? 'off' : 'on';
    if (paused) {
      for (const animation of active) animation.complete();
      active.clear();
      return;
    }
    const vh = innerHeight;
    const mobile = innerWidth < 581;
    // Read layout first; write styles only after all reads.
    const heroRect = hero.getBoundingClientRect();
    const overtureRect = overture.getBoundingClientRect();
    const studioRect = studioShell.getBoundingClientRect();
    const tuningRect = tuning.getBoundingClientRect();
    const modelRect = model.getBoundingClientRect();
    const heroP = clamp(-heroRect.top / (heroRect.height * .85));
    const overtureP = progress(overtureRect, vh);
    const studioP = clamp((vh * .95 - studioRect.top) / (vh * .8));
    const tuningP = progress(tuningRect, vh);
    const modelP = progress(modelRect, vh);
    hero.style.setProperty('--hero-lift', `${heroP * (mobile ? -35 : -105)}px`);
    hero.style.setProperty('--hero-turn', `${heroP * -16}deg`);
    hero.style.setProperty('--hero-zoom', `${1 + heroP * .12}`);
    hero.style.setProperty('--copy-drift', `${heroP * (mobile ? 0 : 55)}px`);
    hero.style.setProperty('--field-turn', `${heroP * 24}deg`);
    overture.style.setProperty('--type-travel', `${(overtureP - .5) * (mobile ? 90 : 240)}px`);
    overture.style.setProperty('--disc-turn', `${overtureP * 140}deg`);
    overture.style.setProperty('--pulse-scale', `${.78 + Math.sin(overtureP * Math.PI) * .22}`);
    studio.style.setProperty('--studio-tilt', `${(1 - studioP) * (mobile ? 7 : 18)}deg`);
    studio.style.setProperty('--studio-scale', `${.87 + studioP * .13}`);
    tuning.style.setProperty('--eq-turn', `${(tuningP - .5) * (mobile ? -2 : -7)}deg`);
    tuning.style.setProperty('--eq-lift', `${(tuningP - .5) * -70}px`);
    model.style.setProperty('--model-lift', `${(modelP - .5) * -65}px`);
    root.style.setProperty('--reading-progress', `${scrollY / Math.max(1, root.scrollHeight - vh)}`);
  }
  function schedule() {
    if (!frame && !disposed && !document.hidden) frame = requestAnimationFrame(render);
  }
  window.addEventListener('scroll', schedule, { passive: true });
  window.addEventListener('resize', schedule, { passive: true });
  document.addEventListener('visibilitychange', schedule);
  const resizeObserver = new ResizeObserver(schedule);
  resizeObserver.observe(document.body);
  schedule();
  return {
    refresh: schedule,
    dispose() {
      disposed = true;
      cancelAnimationFrame(frame);
      headingObserver.disconnect();
      resizeObserver.disconnect();
      active.forEach(animation => animation.complete());
      window.removeEventListener('scroll', schedule);
      window.removeEventListener('resize', schedule);
      document.removeEventListener('visibilitychange', schedule);
    },
  };
}
