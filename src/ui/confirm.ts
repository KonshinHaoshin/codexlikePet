export interface ConfirmOptions {
  title?: string;
  confirmLabel?: string;
  cancelLabel?: string;
}

export interface PromptOptions {
  title?: string;
  confirmLabel?: string;
  cancelLabel?: string;
}

/**
 * Tauri 的 webview（macOS WKWebView）不实现 window.confirm() / window.prompt()：
 * confirm 总是静默返回 false，prompt 总是返回 null。这里用原生 <dialog> 提供
 * 可预测的确认与输入弹窗，样式复用管理页的对话框。
 */
function createDialog(): HTMLDialogElement {
  const dialog = document.createElement("dialog");
  dialog.className = "confirm-dialog";
  const shell = document.createElement("div");
  shell.className = "dialog-shell confirm-shell";
  dialog.append(shell);
  document.body.append(dialog);
  return dialog;
}

function closeDialog(dialog: HTMLDialogElement, value: string): void {
  dialog.returnValue = value;
  dialog.close();
}

function isOpen(dialog: HTMLDialogElement): boolean {
  return dialog.open;
}

export function confirmDialog(message: string, options: ConfirmOptions = {}): Promise<boolean> {
  const dialog = createDialog();
  const shell = dialog.querySelector(".confirm-shell")!;

  const heading = document.createElement("h2");
  heading.textContent = options.title ?? "确认操作";
  const text = document.createElement("p");
  text.className = "confirm-message";
  text.textContent = message;
  const actions = document.createElement("div");
  actions.className = "confirm-actions";

  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.className = "secondary-button";
  cancel.textContent = options.cancelLabel ?? "取消";
  const ok = document.createElement("button");
  ok.type = "button";
  ok.className = "primary-button danger-button";
  ok.textContent = options.confirmLabel ?? "确定";

  actions.append(cancel, ok);
  shell.append(heading, text, actions);

  return new Promise<boolean>((resolve) => {
    const finish = (): void => {
      const value = dialog.returnValue === "yes";
      if (isOpen(dialog)) closeDialog(dialog, dialog.returnValue);
      dialog.remove();
      resolve(value);
    };
    dialog.addEventListener("close", finish, { once: true });
    ok.addEventListener("click", () => closeDialog(dialog, "yes"));
    cancel.addEventListener("click", () => closeDialog(dialog, "no"));
    dialog.addEventListener("click", (event) => {
      if (event.target === dialog) closeDialog(dialog, "no");
    });
    dialog.addEventListener("cancel", (event) => {
      event.preventDefault();
      closeDialog(dialog, "no");
    });
    dialog.showModal();
    ok.focus();
  });
}

export function promptDialog(
  initial: string,
  options: PromptOptions = {},
): Promise<string | null> {
  const dialog = createDialog();
  const shell = dialog.querySelector(".confirm-shell")!;

  const heading = document.createElement("h2");
  heading.textContent = options.title ?? "输入内容";
  const form = document.createElement("form");
  form.className = "confirm-form";
  const input = document.createElement("input");
  input.className = "confirm-input";
  input.value = initial;
  input.setAttribute("autocomplete", "off");
  const actions = document.createElement("div");
  actions.className = "confirm-actions";

  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.className = "secondary-button";
  cancel.textContent = options.cancelLabel ?? "取消";
  const ok = document.createElement("button");
  ok.type = "submit";
  ok.className = "primary-button";
  ok.textContent = options.confirmLabel ?? "确定";

  actions.append(cancel, ok);
  form.append(input, actions);
  shell.append(heading, form);

  return new Promise<string | null>((resolve) => {
    const finish = (): void => {
      const value = dialog.returnValue === "yes" ? input.value : null;
      if (isOpen(dialog)) closeDialog(dialog, dialog.returnValue);
      dialog.remove();
      resolve(value);
    };
    dialog.addEventListener("close", finish, { once: true });
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      closeDialog(dialog, "yes");
    });
    cancel.addEventListener("click", () => closeDialog(dialog, "no"));
    dialog.addEventListener("click", (event) => {
      if (event.target === dialog) closeDialog(dialog, "no");
    });
    dialog.addEventListener("cancel", (event) => {
      event.preventDefault();
      closeDialog(dialog, "no");
    });
    dialog.showModal();
    input.select();
    input.focus();
  });
}