package xyz.tironi.zoen.ui

import org.junit.Assert.*
import org.junit.Test
import xyz.tironi.zoen.core.*

class PermissionScopeTest {
    private fun agent(id: String) = Persona(id, PersonaKind.AGENT, "Zoen", "zoen", "Z", "#123456", null, "", "me", "Me", null, false, true)
    private fun standing(id: String, agent: String, space: String, allow: Boolean) =
        StandingDecisionDto(id, agent(agent), space, "Same chat title", "reversible", "Edit a plan", allow, 1000)
    private fun device(id: String, item: String) =
        DeviceGrantDto(id, item, "Same app title", "Hike", "Same chat title", "location.approximate", "Meet nearby", true, 1000)

    @Test fun selectedAgentAndChatMatchIdsAndRetainBothAllowAndDenyRules() {
        val grants = listOf(standing("allow", "a", "s", true), standing("deny", "a", "s", false), standing("other-agent", "b", "s", true), standing("other-chat", "a", "t", true))
        assertEquals(listOf("allow", "deny"), standingInScope(grants, "a", "s").map { it.grantId })
        assertEquals(3, standingInScope(grants, "a", null).size)
        assertEquals(3, standingInScope(grants, null, "s").size)
        assertEquals(grants, standingInScope(grants, null, null))
    }

    @Test fun appPermissionsUseItemSpaceIdsEvenWhenChatTitlesAreIdentical() {
        val grants = listOf(device("a", "mine"), device("b", "other"), device("c", "deleted"))
        val scopes = mapOf("mine" to PermissionItemScope("s", "agent-a"), "other" to PermissionItemScope("t", "agent-a"))
        assertEquals(listOf("a"), deviceInScope(grants, scopes, "agent-b", "s").map { it.grantId })
        assertEquals(listOf("a", "b"), deviceInScope(grants, scopes, "agent-a", null).map { it.grantId })
        assertTrue(deviceInScope(grants, scopes, "missing", null).isEmpty())
        assertTrue(deviceInScope(grants, scopes, null, "missing").isEmpty())
        assertEquals(grants, deviceInScope(grants, emptyMap(), null, null))
    }
}
