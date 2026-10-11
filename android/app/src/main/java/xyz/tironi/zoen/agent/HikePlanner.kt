package xyz.tironi.zoen.agent

import kotlinx.serialization.json.*
import xyz.tironi.zoen.core.EntryKind
import xyz.tironi.zoen.core.PersonaKind
import xyz.tironi.zoen.core.TimelineEntry

/** Reads only explicit offers and RSVPs. These rules are deterministic on both clients. */
object HikePlanner {
    private val driverCues = listOf("i can drive", "i'll drive", "i’ll drive", "i have a car", "i've got the car", "i’ve got the car", "my car", "room for", "eu dirijo", "tenho carro", "levo o carro", "vou de carro", "posso dirigir", "cabem mais")
    private val inCues = listOf("i'm in", "i’m in", "im in", "count me in", "in!", "i'm down", "i’m down", "down!", "to dentro", "eu vou", "bora", "conta comigo")
    private val demoNeighborhoods = mapOf("Marina" to "Hayes Valley", "Ana" to "Mission", "Enzo" to "Noe Valley", "Lucas" to "Inner Sunset")
    private val route = listOf("Inner Sunset", "Noe Valley", "Mission", "Hayes Valley")

    fun plan(entries: List<TimelineEntry>, since: Long, voters: List<String>, demo: Boolean): String? {
        var driver: String? = null
        val riders = mutableListOf<String>()
        for (entry in entries.filter { it.atMs >= since && it.author.kind == PersonaKind.PERSON }) {
            val message = entry.kind as? EntryKind.Message ?: continue
            val text = " ${folded(message.text)} "
            if (driver == null && driverCues.any { text.contains(folded(it)) }) driver = entry.author.name
            if (inCues.any { text.contains(folded(it)) }) riders.add(entry.author.name)
        }
        val driverName = driver ?: return null
        val people = (riders + voters).distinct().filter { it != driverName }
        if (people.isEmpty()) return null
        // Neighborhoods in the Apple showcase are fixtures. Real accounts must provide a pickup place.
        val locations = if (demo) demoNeighborhoods else emptyMap()
        val places = people.groupBy { locations[it].orEmpty() }
        val start = locations[driverName]
        val sorted = places.keys.sortedWith(compareBy<String> { route.indexOf(it).takeIf { index -> index >= 0 } ?: 99 }.thenBy { it })
        val order = if (start != null && places.containsKey(start)) listOf(start) + sorted.filter { it != start } else sorted
        return buildJsonObject {
            put("driver", driverName)
            putJsonArray("pickups") { order.forEach { place ->
                add(buildJsonObject { put("place", place); putJsonArray("names") { places.getValue(place).forEach { add(it) } } })
            } }
        }.toString()
    }
}
