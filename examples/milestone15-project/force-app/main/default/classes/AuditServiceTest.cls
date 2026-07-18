@IsTest
private class AuditServiceTest {
    @IsTest
    static void formatsEntry() {
        System.assertEquals('release:ready', AuditService.entry('ready'));
    }
}
