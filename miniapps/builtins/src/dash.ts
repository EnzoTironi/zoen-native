/** Client-only Donkey Dash physics, ported from the existing Swift DashGame.
 * Shared scores still enter through the canonical pet_dash_score tool. */
export type DashState = {
  y: number;
  vy: number;
  distance: number;
  speed: number;
  carrots: number;
  obstacles: number[];
  carrotsX: { x: number; h: number }[];
  spawnIn: number;
  carrotIn: number;
  last: number | null;
  over: boolean;
};
export function newDash(): DashState {
  return {
    y: 0,
    vy: 0,
    distance: 0,
    speed: 160,
    carrots: 0,
    obstacles: [],
    carrotsX: [],
    spawnIn: 1.2,
    carrotIn: 0.7,
    last: null,
    over: false,
  };
}
export function jumpDash(state: DashState) {
  if (!state.over && state.y <= 0.5) state.vy = 420;
}
export function stepDash(
  state: DashState,
  now: number,
  width: number,
  random: () => number = Math.random,
): boolean {
  const last = state.last;
  state.last = now;
  if (state.over || last === null) return false;
  const dt = Math.max(0, Math.min(1 / 20, (now - last) / 1000));
  state.speed += dt * 6;
  state.distance += (state.speed * dt) / 18;
  state.vy -= 1300 * dt;
  state.y = Math.max(0, state.y + state.vy * dt);
  if (state.y === 0) state.vy = 0;
  state.obstacles = state.obstacles.map((x) => x - state.speed * dt).filter((x) => x > -30);
  state.carrotsX = state.carrotsX
    .map((c) => ({ ...c, x: c.x - state.speed * dt }))
    .filter((c) => c.x > -30);
  state.spawnIn -= dt;
  state.carrotIn -= dt;
  if (state.spawnIn <= 0) {
    state.obstacles.push(width + 20);
    state.spawnIn = ((1.1 + random()) * 160) / state.speed + 0.5;
  }
  if (state.carrotIn <= 0) {
    state.carrotsX.push({ x: width + 20, h: [0, 46, 70][Math.min(2, Math.floor(random() * 3))] });
    state.carrotIn = 0.8 + random() * 0.8;
  }
  for (const x of state.obstacles)
    if (x < 82 && x + 12 > 42 && state.y < 26) {
      state.over = true;
      return true;
    }
  state.carrotsX = state.carrotsX.filter((c) => {
    if (c.x < 82 && c.x + 10 > 42 && Math.abs(state.y + 12 - c.h - 6) < 24) {
      state.carrots++;
      return false;
    }
    return true;
  });
  return false;
}
