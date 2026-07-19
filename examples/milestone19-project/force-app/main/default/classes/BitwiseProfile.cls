public class BitwiseProfile {
    public static Integer receiverEvaluations = 0;
    public static Integer indexEvaluations = 0;
    public static Integer rightEvaluations = 0;
    public static Integer booleanEvaluations = 0;
    public static BitwiseProfile current;

    public Long accumulator;
    public List<Long> words;

    public BitwiseProfile(Long seed) {
        accumulator = seed;
        words = new List<Long>{seed};
    }

    public static BitwiseProfile selected() {
        receiverEvaluations++;
        return current;
    }

    public static Integer nextIndex() {
        indexEvaluations++;
        return 0;
    }

    public static Integer nextDistance() {
        rightEvaluations++;
        return 4;
    }

    public static Long nextMask() {
        rightEvaluations++;
        return 3L;
    }

    public static Boolean observed(Boolean value) {
        booleanEvaluations++;
        return value;
    }

    public static Long pack(Integer code) {
        return (1L << 40) | (code & 255);
    }

    public static String mutateEveryPlaceOnce() {
        receiverEvaluations = 0;
        indexEvaluations = 0;
        rightEvaluations = 0;
        current = new BitwiseProfile(3L);

        selected().words[nextIndex()] <<= nextDistance();
        selected().accumulator |= nextMask();

        Counter__c counter = new Counter__c();
        counter.Value__c = 5;
        counter.Value__c += 3;

        return current.words[0] + ':' + current.accumulator + ':'
            + counter.Value__c + ':' + receiverEvaluations + ':'
            + indexEvaluations + ':' + rightEvaluations;
    }

    public static String run() {
        booleanEvaluations = 0;
        Boolean eager = observed(false) & observed(true);
        Integer unsignedInteger = -1 >>> 28;
        Long unsignedLong = -1L >>> 60;
        Integer precedence = 1 | 2 ^ 3 & 1;

        Integer arithmetic = 10;
        arithmetic += 2;
        arithmetic *= 3;
        arithmetic -= 4;
        arithmetic /= 2;
        arithmetic %= 5;

        String result = pack(255) + '|' + unsignedInteger + '|'
            + unsignedLong + '|' + precedence + '|' + eager + '|'
            + booleanEvaluations + '|' + arithmetic + '|'
            + mutateEveryPlaceOnce();
        System.debug(result);
        return result;
    }
}
