@IsTest
private class OracleDemoTest {
    @IsTest
    static void confirmsBehavior() {
        OracleDemo.run();
        List<Invoice__c> rows = [SELECT Status__c FROM Invoice__c];
        System.assertEquals(1, rows.size());
        System.assertEquals('prepared', rows[0].Status__c);
    }
}
