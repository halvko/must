/** Runs operations one at a time, in call order; a failure does not stop later ones. */
export function serialized(): <T>(operation: () => Promise<T>) => Promise<T> {
  let tail: Promise<unknown> = Promise.resolve();
  return (operation) => {
    const result = tail.then(operation);
    tail = result.catch(() => undefined);
    return result;
  };
}
