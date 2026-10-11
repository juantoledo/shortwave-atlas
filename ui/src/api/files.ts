// Files the listener saves or opens, the same in the browser and the desktop webview: a download
// of text made in the page, and a file picked with <input type="file">.

/** Hand `text` to the browser as a file called `name` to save. */
export function downloadText(name: string, text: string, type = 'application/json') {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/** A picked file's text; `null` if it is larger than `max` bytes. */
export async function readText(file: File, max: number): Promise<string | null> {
  return file.size > max ? null : file.text();
}
