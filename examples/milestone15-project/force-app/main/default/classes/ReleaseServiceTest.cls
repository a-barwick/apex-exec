@IsTest
private class ReleaseServiceTest {
    @IsTest
    static void preparesPriorityInvoice() {
        Invoice__c invoice = ReleaseService.prepare(125);
        System.assertEquals(125, invoice.Amount__c);
        System.assertEquals('priority', invoice.Status__c);
    }

    @IsTest
    static void preparesStandardInvoice() {
        Invoice__c invoice = ReleaseService.prepare(25);
        System.assertEquals('standard', invoice.Status__c);
    }
}
