package xyz.tironi.zoen.ui

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class DashStateTest {
    @Test fun jumpsDoNotStackAndPauseAndRestorationPreserveTheActualRun() {
        var game = DashState().jump().step(0.0, 320.0).state
        assertEquals(DashState.Phase.Running, game.phase)
        game = game.jump().step(.02, 320.0).state
        assertTrue(game.y > 0)
        assertEquals(game, game.jump())
        val paused = game.pause()
        assertEquals(game.distance, paused.step(100.0, 320.0).state.distance, 0.0)
        val restored = Json.decodeFromString<DashState>(Json.encodeToString(paused))
        assertEquals(paused, restored)
        game = restored.resume().step(101.0, 320.0).state
        assertEquals(paused.distance, game.distance, 0.0)
        assertTrue(game.step(101.02, 320.0).state.distance > game.distance)
    }

    @Test fun aGroundCollisionFinishesOnceAndAnAirborneCarrotCanOnlyBeCollectedOnce() {
        val initial = DashState(phase = DashState.Phase.Running, last = 0.0, obstacles = listOf(70.0),
            food = listOf(DashState.Carrot(60.0, 46.0)))
        val crashed = initial.step(.01, 320.0)
        assertTrue(crashed.finished)
        assertEquals(DashState.Phase.Over, crashed.state.phase)
        assertFalse(crashed.state.step(.02, 320.0).finished)
        val jumped = initial.copy(y = 40.0).step(.01, 320.0)
        assertFalse(jumped.finished)
        assertEquals(1, jumped.collected)
        assertEquals(1, jumped.state.carrots)
        assertTrue(jumped.state.food.isEmpty())
        assertEquals(0, jumped.state.step(.02, 320.0).collected)
    }

    @Test fun aDelayedFrameDoesNotTeleportTheRunnerOrInventElapsedDistance() {
        val game = DashState(phase = DashState.Phase.Running, last = 0.0)
        val delayed = game.step(10.0, 320.0).state
        assertTrue(delayed.distance > 0 && delayed.distance < 1)
        assertTrue(delayed.obstacles.isEmpty())
        assertEquals(delayed.distance, delayed.step(9.0, 320.0).state.distance, 0.0)
    }
}
