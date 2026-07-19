@IsTest
private class M25ProfileDifferenceTest {
    @IsTest
    static void mixedProfilesMatchSalesforce() {
        System.assertEquals(true, M25LegacyProfile.nullIsString());
        System.assertEquals(false, M25CurrentProfile.nullIsString());

        M25VersionProbe__c probe = new M25VersionProbe__c();
        probe.Name = 'oracle';
        insert probe;
        M25VersionProbe__c stored = [
            SELECT LegacyObserved__c
            FROM M25VersionProbe__c
            WHERE Id = :probe.Id
        ];
        System.assertEquals(true, stored.LegacyObserved__c);
    }
}
