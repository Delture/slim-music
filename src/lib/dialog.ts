import { reactive } from 'vue';

export type DialogKind = 'prompt' | 'confirm' | 'alert';
interface DialogState {
  open: boolean;
  kind: DialogKind;
  title: string;
  message: string;
  value: string;
  confirmText: string;
  danger: boolean;
  resolve: ((v: string | boolean | null) => void) | null;
}

export const dialogState = reactive<DialogState>({
  open: false, kind: 'alert', title: '', message: '', value: '',
  confirmText: '确定', danger: false, resolve: null,
});

function show(p: Partial<DialogState> & { kind: DialogKind }): Promise<string | boolean | null> {
  // Settle any dialog still open as cancelled.
  dialogState.resolve?.(p.kind === 'prompt' ? null : false);
  return new Promise((resolve) => {
    Object.assign(dialogState, {
      title: '', message: '', value: '', confirmText: '确定', danger: false,
      ...p, open: true, resolve,
    });
  });
}

export function closeDialog(ok: boolean) {
  const r = dialogState.resolve;
  dialogState.resolve = null;
  dialogState.open = false;
  if (!r) return;
  if (dialogState.kind === 'prompt') r(ok ? dialogState.value : null);
  else r(ok);
}

export const dialog = {
  prompt: (title: string, value = '', confirmText = '确定') =>
    show({ kind: 'prompt', title, value, confirmText }) as Promise<string | null>,
  confirm: (title: string, message = '', opts: { danger?: boolean; confirmText?: string } = {}) =>
    show({ kind: 'confirm', title, message, danger: !!opts.danger, confirmText: opts.confirmText ?? '确定' }) as Promise<boolean>,
  alert: (message: string, title = '出错了') =>
    show({ kind: 'alert', title, message }).then(() => undefined),
};

export function useDialog() { return dialog; }
