/** A closed frame never regains authority when a new frame is initialized. */
export function frameAuthority(checkOwner: () => void) {
  let active = true;
  return {
    check: () => {
      if (!active) throw new Error('Esta interface foi fechada.');
      checkOwner();
    },
    close: () => {
      active = false;
    },
  };
}
