@IsTest
private class M25ProfileDemoTest {
    @IsTest
    static void mixedProfiles() {
        System.assertEquals(
            'true|false|true|1|1|current',
            M25ProfileDemo.run()
        );
    }
}
