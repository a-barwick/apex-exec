@IsTest
private with sharing class M27SecurityOracleTest {
    @IsTest
    static void securityProfilesMatchSalesforce() {
        Profile standard = [
            SELECT Id
            FROM Profile
            WHERE Name = 'Standard User'
            LIMIT 1
        ];
        User restricted = new User();
        restricted.Username = 'm27-security-oracle@example.invalid';
        restricted.Alias = 'm27user';
        restricted.Email = 'm27-security-oracle@example.invalid';
        restricted.EmailEncodingKey = 'UTF-8';
        restricted.LastName = 'M27 Security';
        restricted.LanguageLocaleKey = 'en_US';
        restricted.LocaleSidKey = 'en_US';
        restricted.ProfileId = standard.Id;
        restricted.TimeZoneSidKey = 'America/Los_Angeles';
        insert restricted;

        M27SecureRow__c first = new M27SecureRow__c();
        first.Name = 'First';
        first.Public__c = 'visible';
        first.Secret__c = 'first secret';
        insert first;

        System.runAs(restricted) {
            M27SecureRow__c owned = new M27SecureRow__c();
            owned.Name = 'Owned';
            owned.Public__c = 'visible';
            owned.Secret__c = 'owned secret';
            insert as system owned;

            System.assertEquals(1, M27WithOracle.directCount());
            System.assertEquals(1, M27WithOracle.inheritedCount());
            System.assertEquals(2, M27WithoutOracle.directCount());
            System.assertEquals(2, M27WithoutOracle.inheritedCount());

            Boolean queryDenied = false;
            try {
                List<M27SecureRow__c> denied = [
                    SELECT Id, Public__c
                    FROM M27SecureRow__c
                    WITH USER_MODE
                ];
            } catch (QueryException error) {
                queryDenied = true;
            }
            System.assert(queryDenied);

            List<M27SecureRow__c> systemRows = [
                SELECT Id, Public__c, Secret__c
                FROM M27SecureRow__c
                WITH SYSTEM_MODE
            ];
            System.assertEquals(1, systemRows.size());

            M27SecureRow__c deniedInsert = new M27SecureRow__c();
            deniedInsert.Name = 'Denied';
            Database.SaveResult deniedResult = Database.insert(
                deniedInsert,
                false,
                AccessLevel.USER_MODE
            );
            System.assert(!deniedResult.isSuccess());
            System.assertEquals(
                StatusCode.CANNOT_INSERT_UPDATE_ACTIVATE_ENTITY,
                deniedResult.getErrors()[0].getStatusCode()
            );

            M27SecureRow__c systemInsert = new M27SecureRow__c();
            systemInsert.Name = 'System';
            Database.SaveResult systemResult = Database.insert(
                systemInsert,
                false,
                AccessLevel.SYSTEM_MODE
            );
            System.assert(systemResult.isSuccess());

            Boolean stripDenied = false;
            try {
                Security.stripInaccessible(AccessType.READABLE, systemRows);
            } catch (NoAccessException error) {
                stripDenied = true;
            }
            System.assert(stripDenied);

            SObjectAccessDecision decision = Security.stripInaccessible(
                AccessType.READABLE,
                systemRows,
                false
            );
            System.assertEquals(1, decision.getRecords().size());
            System.assert(
                decision
                    .getRemovedFields()
                    .get('M27SecureRow__c')
                    .contains('Secret__c')
            );
        }
    }
}
