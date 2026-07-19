@IsTest
private class M24PartialDmlTest {
    @IsTest
    static void partialResultsMatchSalesforce() {
        M24DmlAudit.beforeRows = 0;
        M24DmlAudit.afterRows = 0;
        M24Result__c first = row('First', 1, null);
        M24Result__c invalid = row('Invalid', null, null);
        M24Result__c third = row('Third', 3, null);
        Integer statementsBefore = Limits.getDmlStatements();

        List<Database.SaveResult> results = Database.insert(
            new List<M24Result__c>{first, invalid, third},
            false
        );

        System.assertEquals(3, results.size());
        System.assert(results[0].isSuccess());
        System.assert(!results[1].isSuccess());
        System.assert(results[2].isSuccess());
        System.assertNotEquals(null, first.Id);
        System.assertEquals(null, invalid.Id);
        System.assertNotEquals(null, third.Id);
        System.assertEquals(null, results[1].getId());
        System.assertEquals(1, results[1].getErrors().size());
        Database.Error error = results[1].getErrors()[0];
        System.assertEquals(StatusCode.REQUIRED_FIELD_MISSING, error.getStatusCode());
        System.assert(error.getMessage().length() > 0);
        System.assertEquals(new List<String>{'Amount__c'}, error.getFields());
        System.assertEquals(statementsBefore + 1, Limits.getDmlStatements());
        System.assertEquals(5, M24DmlAudit.beforeRows);
        System.assertEquals(4, M24DmlAudit.afterRows);
        System.assertEquals(2, [
            SELECT COUNT() FROM M24Result__c
        ]);

        M24Result__c seed = row('Seed', 4, 'seed');
        upsert seed External_Key__c;
        M24Result__c updateRow = row('Updated', 5, 'SEED');
        M24Result__c insertRow = row('Inserted', 6, 'new');
        List<Database.UpsertResult> upserts = Database.upsert(
            new List<M24Result__c>{updateRow, insertRow},
            M24Result__c.Fields.External_Key__c,
            false
        );
        System.assert(upserts[0].isSuccess());
        System.assert(!upserts[0].isCreated());
        System.assert(upserts[1].isSuccess());
        System.assert(upserts[1].isCreated());
        System.assertEquals(seed.Id, updateRow.Id);
        System.assertNotEquals(null, insertRow.Id);
        System.assertEquals(4, [
            SELECT COUNT() FROM M24Result__c
        ]);
    }

    private static M24Result__c row(
        String name,
        Integer amount,
        String externalKey
    ) {
        M24Result__c value = new M24Result__c();
        value.Name = name;
        value.Amount__c = amount;
        value.External_Key__c = externalKey;
        return value;
    }
}
