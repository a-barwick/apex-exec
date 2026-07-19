@IsTest
private class NestedCompatibilityTest {
    @IsTest
    static void nestedTypesEnumsAndInitializers() {
        System.assertEquals(
            'Running|1|3|7|NestedCompatibility.Job|1',
            NestedCompatibility.run()
        );
        System.assertEquals(1, NestedCompatibility.initializationCount);
        System.assertEquals(2, new NestedCompatibility.BaseJob(2).score());
    }

    @IsTest
    static void customExceptionsRemainTyped() {
        System.assertEquals(
            'NestedCompatibility.LocalFailure:typed',
            NestedCompatibility.catchCustomFailure()
        );
    }
}
