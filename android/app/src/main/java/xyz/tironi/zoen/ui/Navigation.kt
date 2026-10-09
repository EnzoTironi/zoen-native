package xyz.tironi.zoen.ui

import androidx.navigation3.runtime.NavKey
import kotlinx.serialization.Serializable

@Serializable data object Home : NavKey
@Serializable data class Chat(val id: String, val message: String? = null) : NavKey
@Serializable data class Item(val id: String) : NavKey
@Serializable data class Agent(val id: String) : NavKey
@Serializable data class Person(val id: String) : NavKey
@Serializable data class Participants(val id: String) : NavKey
@Serializable data class Thread(val space: String, val root: String) : NavKey
@Serializable data object Files : NavKey
@Serializable data object Agents : NavKey
@Serializable data object Context : NavKey
@Serializable data object Search : NavKey
@Serializable data object History : NavKey
@Serializable data object Permissions : NavKey
@Serializable data object NewChat : NavKey
@Serializable data object NewSpace : NavKey
@Serializable data class Join(val code: String = "") : NavKey
@Serializable data class Request(val id: String) : NavKey

enum class Tab { Chats, Spaces, Activity }
