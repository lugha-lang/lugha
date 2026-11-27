// Calls tests/jni/words.la through the Kotlin class `--jni` writes.
import com.example.Words
import kotlin.concurrent.thread

fun main() {
    println(Words.wordCount("the quick brown fox"))
    println(Words.greet("Ada"))
    println(Words.greet(null))
    val text = "é𝄞a"
    println(Words.echo(text) == text)
    println(Words.echo(text).length)
    println(Words.echo("\uD800x") == "�x")
    println("${Words.isBig(7)} ${Words.isBig(200)}")
    println(Words.upper('a'.code))
    println(Words.same(1234L))
    println(Words.half(3.0))
    Words.nothing()
    try {
        Words.pick(7)
        println("not reached")
    } catch (e: Words.Panic) {
        println("Panic: ${e.message}")
    }
    try {
        Words.echo("a\u0000b")
        println("not reached")
    } catch (e: IllegalArgumentException) {
        println("refused: ${e.message}")
    }
    println(Words.pick(1))
    val threads = (1..4).map {
        thread {
            repeat(50) { check(Words.wordCount("a b c") == 3L) }
        }
    }
    threads.forEach { it.join() }
    println("four threads answered")
}
