@IsTest
private class BitwiseProfileTest {
    @IsTest
    static void coversIntegralBitwiseAndShiftSemantics() {
        System.assertEquals(1099511628031L, BitwiseProfile.pack(255));
        System.assertEquals(-2147483648, 1 << 31);
        System.assertEquals(1, 1 << 32);
        System.assertEquals(2147483647, -1 >>> 1);
        System.assertEquals(-1, -1 >> 31);
        System.assertEquals(1L, 1L << 64);
        System.assertEquals(9223372036854775807L, -1L >>> 1);
        System.assertEquals(-2, ~1);
        System.assertEquals(-2L, ~1L);
    }

    @IsTest
    static void booleanBitwiseOperatorsEvaluateBothOperands() {
        BitwiseProfile.booleanEvaluations = 0;
        System.assertEquals(false, BitwiseProfile.observed(false) & BitwiseProfile.observed(true));
        System.assertEquals(2, BitwiseProfile.booleanEvaluations);

        BitwiseProfile.booleanEvaluations = 0;
        System.assertEquals(true, BitwiseProfile.observed(true) | BitwiseProfile.observed(false));
        System.assertEquals(2, BitwiseProfile.booleanEvaluations);

        BitwiseProfile.booleanEvaluations = 0;
        System.assertEquals(true, BitwiseProfile.observed(true) ^ BitwiseProfile.observed(false));
        System.assertEquals(2, BitwiseProfile.booleanEvaluations);
    }

    @IsTest
    static void compoundPlacesEvaluateReceiverIndexAndRightOnce() {
        System.assertEquals(
            '48:3:8:2:1:2',
            BitwiseProfile.mutateEveryPlaceOnce()
        );
    }

    @IsTest
    static void longBoundsAndCompoundArithmeticAreStable() {
        Long minimum = -9223372036854775808L;
        Long maximum = 9223372036854775807L;
        minimum++;
        maximum--;
        System.assertEquals(-9223372036854775807L, minimum);
        System.assertEquals(9223372036854775806L, maximum);

        System.assertEquals(
            '1099511628031|15|15|3|false|2|1|48:3:8:2:1:2',
            BitwiseProfile.run()
        );
    }
}
