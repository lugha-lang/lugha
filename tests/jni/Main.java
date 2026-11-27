// Calls tests/jni/words.la through the Java class `--jni-lang java` writes.
import com.example.Words;

public class Main {
    public static void main(String[] args) {
        System.out.println(Words.wordCount("the quick brown fox"));
        System.out.println(Words.greet(null));
        System.out.println(Words.echo("é𝄞a").length());
        try {
            Words.echo(null);
            System.out.println("not reached");
        } catch (NullPointerException e) {
            System.out.println("null: " + e.getMessage());
        }
        try {
            Words.pick(7);
            System.out.println("not reached");
        } catch (Words.Panic e) {
            System.out.println("Panic: " + e.getMessage());
        }
        System.out.println(Words.pick(2));
    }
}
