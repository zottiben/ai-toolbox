// Inject via chrome-devtools evaluate_script, wrapped in `() => { ...this... }`.
// CDP input does not move the real macOS pointer, so a screen recording of
// CDP-driven clicks shows nothing causing the changes. This draws one instead.
//
// Exposes window.__demo = { wait, moveTo, pulse, click, type, cursor, log }.
// Write your choreography as window.__demo.run = async () => { ... } and call it
// WITHOUT awaiting, so the tool call returns while the demo plays.

document.getElementById('__demoCursor')?.remove();
document.getElementById('__demoRipple')?.remove();
document.getElementById('__demoStyle')?.remove();

const style = document.createElement('style');
style.id = '__demoStyle';
style.textContent = `
  #__demoCursor{position:fixed;left:0;top:0;width:22px;height:22px;z-index:2147483647;
    pointer-events:none;transition:transform .5s cubic-bezier(.4,0,.2,1);will-change:transform;
    filter:drop-shadow(0 1px 2px rgba(0,0,0,.4))}
  #__demoRipple{position:fixed;left:-13px;top:-13px;width:26px;height:26px;border-radius:50%;
    background:rgba(0,0,0,.28);z-index:2147483646;pointer-events:none;opacity:0;
    transition:transform .5s cubic-bezier(.4,0,.2,1)}
  #__demoRipple.go{animation:__demoPulse .45s ease-out}
  @keyframes __demoPulse{0%{opacity:.9}100%{opacity:0}}
`;
document.head.appendChild(style);

const cursor = document.createElement('div');
cursor.id = '__demoCursor';
cursor.innerHTML = `<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
  <path d="M5 2l14 9-6 1.2 3.2 6.3-2.6 1.3L10.4 13 5 17.6z"
        fill="#fff" stroke="#111" stroke-width="1.4" stroke-linejoin="round"/></svg>`;
document.body.appendChild(cursor);

const ripple = document.createElement('div');
ripple.id = '__demoRipple';
document.body.appendChild(ripple);

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

const moveTo = async (el, settle = 700) => {
  if (!el) {
    throw new Error('demo: target element not found');
  }

  const rect = el.getBoundingClientRect();
  const x = rect.left + rect.width / 2;
  const y = rect.top + rect.height / 2;

  cursor.style.transform = `translate(${x}px, ${y}px)`;
  ripple.style.transform = `translate(${x}px, ${y}px)`;

  await wait(settle);
};

const pulse = async () => {
  ripple.classList.remove('go');
  void ripple.offsetWidth;
  ripple.classList.add('go');
  await wait(160);
};

const click = async (el, settle) => {
  await moveTo(el, settle);
  await pulse();
  el.click();
  await wait(120);
};

const type = async (el, text, delay = 90) => {
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;

  el.focus();
  setter.call(el, '');
  el.dispatchEvent(new Event('input', { bubbles: true }));

  for (const ch of text) {
    setter.call(el, el.value + ch);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    await wait(delay);
  }
};

const hover = async (el, settle = 330) => {
  await moveTo(el, settle);
  el.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }));
  el.dispatchEvent(new MouseEvent('mouseenter', { bubbles: true }));
};

// Pages often scroll inside a container, so window.scrollTo silently does nothing.
const scrollIntoPosition = (el, topOffset = 250) => {
  let scroller = el.parentElement;

  while (scroller) {
    const s = getComputedStyle(scroller);

    if (/(auto|scroll)/.test(s.overflowY) && scroller.scrollHeight > scroller.clientHeight) {
      break;
    }

    scroller = scroller.parentElement;
  }

  const target = scroller ?? document.documentElement;
  target.scrollTop += el.getBoundingClientRect().top - topOffset;

  return Math.round(el.getBoundingClientRect().top);
};

cursor.style.transform = 'translate(50vw, 30vh)';
ripple.style.transform = 'translate(50vw, 30vh)';

window.__demo = { wait, moveTo, pulse, click, type, hover, scrollIntoPosition, cursor, log: [] };

return 'demo cursor installed';
