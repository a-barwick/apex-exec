@IsTest
private class M27SharingDemoTest {
    @IsTest
    static void sharingPropagation() {
        System.assertEquals('1|1|2|2|2', M27SharingDemo.run());
    }
}
