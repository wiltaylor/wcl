/* The editable surface's mount-scoped reference.

   An editable surface hands its host ONE handle covering everything the
   host may ask of it — navigate, read the preview document, re-decorate,
   and the merged/site facts the visibility editor needs — and releases it
   on unmount. A released handle is inert: every member is a no-op returning
   undefined, so a host that kept the reference (or a callback that outlived
   the component) cannot act on a surface that is no longer there. */

/**
 * Wrap `impl` (a plain object of methods) in a handle whose members stop
 * working once `release()` runs. The handle carries exactly `impl`'s
 * members — a host asks the surface for things, it doesn't interrogate the
 * reference.
 *
 * @returns {{ handle: object, release: () => void }}
 */
export function createSurfaceHandle(impl) {
  let live = true;
  const handle = {};
  for (const [name, fn] of Object.entries(impl)) {
    handle[name] = (...args) => (live ? fn(...args) : undefined);
  }
  return {
    handle,
    release() {
      live = false;
    },
  };
}
