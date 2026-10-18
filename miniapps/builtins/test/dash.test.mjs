import assert from 'node:assert/strict';
import test from 'node:test';
import { newDash, jumpDash, stepDash } from '../src/dash.ts';
test('Dash ground collision ends the run while a correctly timed jump clears the same fence', () => {
  const ground = newDash();
  ground.last = 0;
  ground.obstacles = [60];
  assert.equal(
    stepDash(ground, 50, 360, () => 0.5),
    true,
  );
  const airborne = newDash();
  airborne.last = 0;
  airborne.y = 38;
  airborne.vy = 250;
  airborne.obstacles = [60];
  assert.equal(
    stepDash(airborne, 50, 360, () => 0.5),
    false,
  );
  assert.equal(airborne.over, false);
});
test('Dash collects each carrot once and clamps an interrupted frame instead of fabricating elapsed distance', () => {
  const game = newDash();
  game.last = 0;
  game.carrotsX = [{ x: 60, h: 0 }];
  stepDash(game, 60_000, 360, () => 0.5);
  assert.equal(game.carrots, 1);
  assert.equal(game.carrotsX.length, 0);
  assert.ok(game.distance < 1);
  stepDash(game, 60_016, 360, () => 0.5);
  assert.equal(game.carrots, 1);
});
test('Dash ignores repeated airborne jumps and finished-run input', () => {
  const game = newDash();
  jumpDash(game);
  assert.equal(game.vy, 420);
  game.y = 30;
  game.vy = 100;
  jumpDash(game);
  assert.equal(game.vy, 100);
  game.over = true;
  game.y = 0;
  jumpDash(game);
  assert.equal(game.vy, 100);
});
