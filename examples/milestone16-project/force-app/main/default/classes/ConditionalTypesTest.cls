@IsTest
private class ConditionalTypesTest {
    @IsTest
    static void coversBothConditionalAndRuntimeTypeBranches() {
        System.assertEquals('primary:String', ConditionalTypes.describe(true, 'value'));
        System.assertEquals(
            'secondary:Other',
            ConditionalTypes.describe(false, new List<String>{'value'})
        );
    }
}
