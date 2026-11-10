import { nextTick, onDeactivated, onMounted, onUnmounted, type Ref } from 'vue';

const PAD = 8;

/** 应用内右键菜单的关闭逻辑：点击菜单外任意处、Esc、滚动、切页/失焦时关闭。 */
export function useAppMenuDismiss(
  menu: { show: boolean; x: number; y: number },
  menuEl: Ref<HTMLElement | null>,
) {
  function closeMenu() {
    menu.show = false;
  }

  function clamp() {
    const node = menuEl.value;
    if (!node || !menu.show) return;
    const r = node.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    if (r.right > vw - PAD) menu.x = Math.max(PAD, vw - r.width - PAD);
    if (r.bottom > vh - PAD) menu.y = Math.max(PAD, vh - r.height - PAD);
  }

  function openMenuAt(e: MouseEvent) {
    menu.x = e.clientX;
    menu.y = e.clientY;
    menu.show = true;
    void nextTick(clamp);
  }

  function isInside(t: EventTarget | null) {
    return !!(t instanceof Node && menuEl.value?.contains(t));
  }

  function onPointer(e: Event) {
    if (!menu.show || isInside(e.target)) return;
    closeMenu();
  }

  function onKey(e: KeyboardEvent) {
    if (!menu.show || e.key !== 'Escape') return;
    e.preventDefault();
    closeMenu();
  }

  function onScroll(e: Event) {
    if (!menu.show || isInside(e.target)) return;
    closeMenu();
  }

  onMounted(() => {
    document.addEventListener('pointerdown', onPointer, true);
    document.addEventListener('contextmenu', onPointer, true);
    window.addEventListener('keydown', onKey, true);
    window.addEventListener('scroll', onScroll, true);
    window.addEventListener('blur', closeMenu);
    window.addEventListener('resize', closeMenu);
  });
  // KeepAlive 停用时 Teleport 到 body 的菜单仍会留在页面上，必须关掉
  onDeactivated(closeMenu);
  onUnmounted(() => {
    document.removeEventListener('pointerdown', onPointer, true);
    document.removeEventListener('contextmenu', onPointer, true);
    window.removeEventListener('keydown', onKey, true);
    window.removeEventListener('scroll', onScroll, true);
    window.removeEventListener('blur', closeMenu);
    window.removeEventListener('resize', closeMenu);
    closeMenu();
  });

  return { openMenuAt, closeMenu };
}
