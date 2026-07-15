public class Calculator {
    public static Integer add(Integer left, Integer right) {
        if (left < 0) {
            return right;
        }
        return left + right;
    }
}
