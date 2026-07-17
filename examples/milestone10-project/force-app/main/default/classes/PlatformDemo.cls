public class PlatformDemo {
    public static void run() {
        Date launch = Date.newInstance(2026, 7, 17);
        Datetime observed = Datetime.valueOfGmt('2026-07-17 09:30:00');
        Time reminder = Time.newInstance(9, 45, 0, 0);
        Decimal total = (10.25 + 2).setScale(2);

        Blob payload = Blob.valueOf('milestone-10');
        String encoded = EncodingUtil.base64Encode(payload);
        Pattern version = Pattern.compile('v([0-9]+)');
        Matcher match = version.matcher('v10');
        match.find();

        Map<String,Object> decoded =
            (Map<String,Object>) JSON.deserializeUntyped(
                '{"milestone":10,"ready":true}'
            );
        Map<String,SObjectType> objects = Schema.getGlobalDescribe();
        DescribeSObjectResult invoice = objects.get('Invoice__c').getDescribe();

        System.debug(
            launch.addDays(1).format() + ' | ' +
            observed.addMinutes(30).format() + ' | ' +
            reminder.addMinutes(15).hour() + ' | ' +
            total + ' | ' + encoded + ' | ' + match.group(1) + ' | ' +
            decoded.get('ready') + ' | ' + invoice.getKeyPrefix()
        );
    }
}
