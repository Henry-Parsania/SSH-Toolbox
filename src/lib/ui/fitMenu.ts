// Keeps a pop-up menu on screen. Put it on a `position: fixed` menu with the place where the
// pointer was: the menu is moved up or left if it would run off the window, and made scrollable
// if it is taller than the window.

const GAP = 8;

export function fitMenu(node: HTMLElement, at: { x: number; y: number }) {
  function place(point: { x: number; y: number }) {
    node.style.maxHeight = `${window.innerHeight - GAP * 2}px`;
    node.style.overflowY = "auto";
    const box = node.getBoundingClientRect();
    const left = Math.max(GAP, Math.min(point.x, window.innerWidth - box.width - GAP));
    const top = Math.max(GAP, Math.min(point.y, window.innerHeight - box.height - GAP));
    node.style.left = `${left}px`;
    node.style.top = `${top}px`;
  }
  place(at);
  return { update: place };
}
