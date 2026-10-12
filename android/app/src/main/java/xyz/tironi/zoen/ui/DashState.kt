package xyz.tironi.zoen.ui

import kotlinx.serialization.Serializable
import kotlin.math.abs
import kotlin.math.max
import kotlin.random.Random

@Serializable
internal data class DashState(
    val phase: Phase = Phase.Ready, val y: Double = 0.0, val velocity: Double = 0.0,
    val distance: Double = 0.0, val speed: Double = 160.0, val carrots: Int = 0,
    val obstacles: List<Double> = emptyList(), val food: List<Carrot> = emptyList(),
    val plusOnes: List<PlusOne> = emptyList(), val last: Double? = null,
    val spawnIn: Double = 1.2, val carrotIn: Double = .7, val frame: Int = 0,
    val toast: Toast? = null, val toastUntil: Double = 0.0,
) {
    @Serializable enum class Phase { Ready, Running, Paused, Over }
    @Serializable enum class Toast { Carrots, Jump }
    @Serializable data class Carrot(val x: Double, val height: Double)
    @Serializable data class PlusOne(val x: Double, val height: Double, val time: Double)
    data class Step(val state: DashState, val finished: Boolean = false, val collected: Int = 0)

    fun jump(): DashState = when {
        phase == Phase.Ready || phase == Phase.Over -> DashState(phase = Phase.Running)
        phase == Phase.Running && y <= .5 -> copy(velocity = 420.0)
        else -> this
    }
    fun pause() = if (phase == Phase.Running) copy(phase = Phase.Paused, last = null) else this
    fun resume() = if (phase == Phase.Paused) copy(phase = Phase.Running, last = null) else this

    fun step(now: Double, width: Double, random: Random = Random.Default): Step {
        if (phase != Phase.Running || last == null) return Step(copy(last = now))
        val dt = (now - last).coerceIn(0.0, 1.0 / 20)
        val speed = speed + dt * 6
        var velocity = velocity - 1300 * dt
        val y = max(0.0, y + velocity * dt)
        if (y == 0.0) velocity = 0.0
        val obstacles = obstacles.map { it - speed * dt }.filter { it > -30 }.toMutableList()
        val food = food.map { it.copy(x = it.x - speed * dt) }.filter { it.x > -30 }.toMutableList()
        var spawnIn = spawnIn - dt
        var carrotIn = carrotIn - dt
        if (spawnIn <= 0) { obstacles.add(width + 20); spawnIn = random.nextDouble(1.1, 2.1) * 160 / speed + .5 }
        if (carrotIn <= 0) { food.add(Carrot(width + 20, listOf(0.0, 46.0, 70.0)[random.nextInt(3)])); carrotIn = random.nextDouble(.8, 1.6) }
        val advanced = copy(y = y, velocity = velocity, speed = speed, distance = distance + speed * dt / 18,
            obstacles = obstacles, food = food, spawnIn = spawnIn, carrotIn = carrotIn, last = now, frame = frame + 1)
        if (y < 26 && obstacles.any { it < 82 && it + 12 > 42 }) return Step(advanced.copy(phase = Phase.Over), finished = true)
        val caught = food.filter { it.x < 82 && it.x + 10 > 42 && abs(y + 12 - it.height - 6) < 24 }
        val plus = plusOnes.filter { now - it.time < .8 } + caught.map { PlusOne(it.x, it.height, now) }
        var toast = toast
        var until = toastUntil
        if (caught.isNotEmpty() && (carrots + 1..carrots + caught.size).any { it % 5 == 0 }) { toast = Toast.Carrots; until = now + 1.4 }
        if (y > 60 && (toast == null || until < now) && advanced.frame % 90 == 0) { toast = Toast.Jump; until = now + 1 }
        return Step(advanced.copy(food = food.filterNot { it in caught }, carrots = carrots + caught.size,
            plusOnes = plus, toast = toast, toastUntil = until), collected = caught.size)
    }
}
