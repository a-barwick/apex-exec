@IsTest
private class CalculatorTest {
    private static Integer setupRuns = 0;

    @TestSetup
    static void setupData() {
        setupRuns++;
    }

    @IsTest
    static void addsPositiveValues() {
        System.assertEquals(1, setupRuns);
        System.assertEquals(5, Calculator.add(2, 3));
    }

    @IsTest
    static void handlesNegativeValues() {
        System.assertEquals(1, setupRuns);
        System.assertEquals(3, Calculator.add(-2, 3));
    }
}
