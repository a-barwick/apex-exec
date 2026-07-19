@IsTest
private class NullAwareProfileTest {
    @IsTest
    static void coversNullAndPresentNavigationPaths() {
        NullAwareProfile named = new NullAwareProfile('  Grace  ');
        NullAwareProfile unnamed = new NullAwareProfile(null);

        System.assertEquals(
            'GRACE',
            NullAwareProfile.normalizedName(named, 'UNKNOWN')
        );
        System.assertEquals(
            'UNKNOWN',
            NullAwareProfile.normalizedName(unnamed, 'UNKNOWN')
        );
        System.assertEquals(
            'UNKNOWN',
            NullAwareProfile.normalizedName(null, 'UNKNOWN')
        );

        NullAwareProfile manager = new NullAwareProfile('Ada');
        named.manager = manager;
        System.assertEquals('Ada', NullAwareProfile.managerName(named));
        System.assertEquals('NO MANAGER', NullAwareProfile.managerName(unnamed));
        System.assertEquals('NO MANAGER', NullAwareProfile.managerName(null));
    }

    @IsTest
    static void provesReceiverOnceAndArgumentsLazy() {
        System.assertEquals('SKIPPED', NullAwareProfile.evaluatedOnce(false));
        System.assertEquals(1, NullAwareProfile.receiverEvaluations);
        System.assertEquals(0, NullAwareProfile.argumentEvaluations);

        System.assertEquals('selected-1', NullAwareProfile.evaluatedOnce(true));
        System.assertEquals(2, NullAwareProfile.receiverEvaluations);
        System.assertEquals(1, NullAwareProfile.argumentEvaluations);
    }

    @IsTest
    static void nullAwareSingleRecordQueriesHandleEmptyAndPresentResults() {
        System.assertEquals('NO INVOICE', NullAwareProfile.firstInvoiceName());

        try {
            NullAwareProfile.strictFirstInvoice();
            System.assert(false, 'ordinary single-record SOQL should remain strict');
        } catch (QueryException expected) {
            System.assert(expected.getMessage().contains('returned 0 records'));
        }

        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'INV-001';
        insert invoice;

        System.assertEquals('INV-001', NullAwareProfile.firstInvoiceName());
    }

    @IsTest
    static void runsTheMilestoneExample() {
        System.assertEquals(
            'GRACE|Ada|SKIPPED|selected-1',
            NullAwareProfile.run()
        );
    }
}
