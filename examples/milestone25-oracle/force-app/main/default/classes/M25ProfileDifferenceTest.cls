@IsTest
private class M25ProfileDifferenceTest {
    @IsTest
    static void mixedProfilesMatchSalesforce() {
        System.assertEquals(true, M25LegacyProfile.nullIsString());
        System.assertEquals(false, M25CurrentProfile.nullIsString());

        M25VersionProbe__c probe = new M25VersionProbe__c();
        probe.Name = 'oracle';
        insert probe;
        System.assertEquals(true, probe.LegacyObserved__c);
    }
}
