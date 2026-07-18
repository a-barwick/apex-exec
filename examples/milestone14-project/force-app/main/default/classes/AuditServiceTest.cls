@IsTest
private class AuditServiceTest {
    @IsTest
    static void formatsEntry() {
        System.assertEquals('audit:ready', AuditService.entry('ready'));
    }
}
