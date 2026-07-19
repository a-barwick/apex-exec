public class M25Legacy {
    public static Integer evaluations = 0;

    private static Object nextNull() {
        evaluations++;
        return null;
    }

    public static Boolean nullIsString() {
        return nextNull() instanceof String;
    }
}
