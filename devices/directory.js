const form = document.querySelector('.filters');
if (form) {
  const cards = [...document.querySelectorAll('.device-card')];
  const search = document.querySelector('#device-search');
  const category = document.querySelector('#device-category');
  const brand = document.querySelector('#device-brand');
  const platform = document.querySelector('#device-platform');
  const status = document.querySelector('#device-status');
  const build = document.querySelector('#device-build');
  function filter() {
    const terms = search.value.trim().toLowerCase().split(/\s+/).filter(Boolean);
    let count = 0;
    for (const card of cards) {
      const matches = terms.every(term => card.dataset.search.includes(term)) &&
        (!brand.value || card.dataset.brand === brand.value) &&
        (!category.value || card.dataset.category === category.value) &&
        (!build.value || card.dataset.availability === build.value) &&
        (!status.value || card.dataset[platform.value].split(' ').includes(status.value));
      card.hidden = !matches;
      if (matches) count++;
    }
    document.querySelector('#result-count').textContent = `${count} ${count === 1 ? 'model' : 'models'}`;
    document.querySelector('.empty-state').hidden = count !== 0;
  }
  form.hidden = false;
  form.addEventListener('submit', event => event.preventDefault());
  form.addEventListener('input', filter);
  form.addEventListener('change', filter);
  form.addEventListener('reset', () => requestAnimationFrame(filter));
}
