public class M25Current {
    public static Integer evaluations = 0;

    private static Object nextNull() {
        evaluations++;
        return null;
    }

    public static Boolean nullIsString() {
        return nextNull() instanceof String;
    }

    public static String currentSyntax() {
        String value = null;
        return value ?? 'current';
    }
}
