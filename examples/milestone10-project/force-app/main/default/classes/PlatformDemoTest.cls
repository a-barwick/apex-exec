@IsTest
private class PlatformDemoTest {
    @IsTest
    static void valueTypesAreDeterministic() {
        Date dateValue = Date.valueOf('2024-02-29');
        System.assertEquals('2025-02-28', dateValue.addYears(1).format());
        System.assertEquals('2024-03-29', dateValue.addMonths(1).format());
        System.assertEquals(2, dateValue.month());
        System.assertEquals(2024, dateValue.year());
        System.assertEquals(29, dateValue.day());
        System.assertEquals(1, dateValue.daysBetween(dateValue.addDays(1)));

        Datetime instant = Datetime.valueOfGmt('2025-01-01 00:00:00');
        System.assertEquals(1735689600000, instant.getTime());
        System.assertEquals('2025-01-01', instant.dateGmt().format());
        System.assertEquals(1, instant.addHours(1).timeGmt().hour());
        System.assertEquals(2, instant.addDays(1).date().day());
        System.assertEquals(1, instant.addMinutes(1).time().minute());
        System.assertEquals(1, instant.addSeconds(1).time().second());

        Time timeValue = Time.valueOf('23:59:59.500');
        System.assertEquals(0, timeValue.addMilliseconds(500).hour());
        System.assertEquals(0, timeValue.addSeconds(1).second());
        System.assertEquals(0, timeValue.addMinutes(1).minute());
        System.assertEquals(0, timeValue.addHours(1).hour());
        System.assertEquals(500, timeValue.millisecond());

        Decimal amount = 12.345;
        System.assertEquals('12.35', amount.setScale(2).toString());
        System.assertEquals(3, amount.scale());
        System.assertEquals(12.345, amount.abs());
        System.assertEquals(2.5, Decimal.valueOf('5.0') / 2);
        System.assertEquals(5.5, 2 + 3.5);
    }

    @IsTest
    static void dataUtilitiesRoundTrip() {
        Id accountId = Id.valueOf('001000000000001');
        System.assertEquals(15, accountId.to15().length());
        System.assertEquals(18, accountId.to18().length());

        Blob bytes = Blob.valueOf('Apex Exec');
        String encoded = EncodingUtil.base64Encode(bytes);
        System.assertEquals('Apex Exec', EncodingUtil.base64Decode(encoded).toString());
        System.assertEquals(9, bytes.size());

        Map<String,Object> value =
            (Map<String,Object>) JSON.deserializeUntyped(
                '{"name":"local","count":2,"flags":[true,false]}'
            );
        System.assertEquals('local', (String) value.get('name'));
        System.assertEquals(2, (Integer) value.get('count'));
        System.assert(JSON.serializePretty(value).contains('\n'));
        System.assertEquals(
            '{"name":"local","count":2,"flags":[true,false]}',
            JSON.serialize(value)
        );
    }

    @IsTest
    static void regexDescribeAndContextWork() {
        Pattern compiledPattern = Pattern.compile('([A-Z]+)-([0-9]+)');
        Matcher matcher = compiledPattern.matcher('ABC-42');
        System.assert(matcher.matches());
        System.assertEquals('ABC', matcher.group(1));
        System.assertEquals(4, matcher.start(2));
        System.assertEquals(6, matcher.end(2));
        Matcher finder = Pattern.compile('[0-9]+').matcher('a1 b22');
        System.assert(finder.find());
        System.assertEquals('1', finder.group());
        System.assert(finder.find());
        System.assertEquals('22', finder.group());

        Map<String,SObjectType> objects = Schema.getGlobalDescribe();
        DescribeSObjectResult invoice = objects.get('Invoice__c').getDescribe();
        System.assertEquals('Invoice__c', invoice.getName());
        System.assert(invoice.isCustom());
        System.assertEquals(3, invoice.getKeyPrefix().length());

        System.assert(Test.isRunningTest());
        System.assertEquals('005000000000001AAA', UserInfo.getUserId().to18());
        System.assertEquals('local@example.invalid', UserInfo.getUserName());
        System.assertEquals(System.now(), Datetime.now());
        System.assertEquals(System.today(), Date.today());
        PlatformDemo.run();
    }

    @IsTest
    static void limitsResetInsideTestWindow() {
        System.assertEquals(0, Limits.getQueries());
        System.assertEquals(0, Limits.getDmlStatements());
        Test.startTest();
        System.assertEquals(0, Limits.getQueries());
        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'Local';
        insert invoice;
        List<Invoice__c> invoices = [SELECT Id, Name FROM Invoice__c];
        System.assertEquals(1, invoices.size());
        System.assertEquals(1, Limits.getQueries());
        System.assertEquals(1, Limits.getDmlStatements());
        System.assertEquals(100, Limits.getLimitQueries());
        System.assertEquals(150, Limits.getLimitDmlStatements());
        System.assertEquals(100, Limits.getLimitCallouts());
        Test.stopTest();
    }
}
