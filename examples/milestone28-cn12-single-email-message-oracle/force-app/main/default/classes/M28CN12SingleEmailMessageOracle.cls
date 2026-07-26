public class M28CN12SingleEmailMessageOracle {
    private static Messaging.SingleEmailMessage message() {
        Messaging.SingleEmailMessage message =
            new Messaging.SingleEmailMessage();
        message.setSubject('M28 CN12');
        message.setHtmlBody('<p>deterministic</p>');
        message.setTargetObjectId(UserInfo.getUserId());
        message.setSaveAsActivity(false);
        message.setToAddresses(
            new List<String>{ 'nobody@example.invalid' }
        );
        return message;
    }

    public static void run() {
        Messaging.SingleEmailMessage message = message();
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|subject|' + message.getSubject()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|htmlBody|' + message.getHtmlBody()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|targetSet|' +
            (message.getTargetObjectId() == UserInfo.getUserId())
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|saveAsActivity|' +
            message.getSaveAsActivity()
        );
        System.debug(
            'APEX_EXEC_ORACLE_VALUE|toAddresses|' +
            String.join(message.getToAddresses(), ',')
        );
    }

    public static void exerciseLocalSend() {
        Messaging.SingleEmailMessage message = message();
        System.Messaging.reserveSingleEmailCapacity(1);
        List<Messaging.SendEmailResult> results =
            Messaging.sendEmail(
                new List<Messaging.SingleEmailMessage>{ message }
            );
        System.debug(
            'APEX_EXEC_LOCAL_VALUE|success|' + results[0].success
        );
        System.debug(
            'APEX_EXEC_LOCAL_VALUE|errors|' + results[0].errors.size()
        );
    }

    public static void exerciseLocalFailure() {
        List<Messaging.SendEmailResult> results =
            Messaging.sendEmail(
                new List<Messaging.SingleEmailMessage>{ message() }
            );
        System.debug(
            'APEX_EXEC_LOCAL_VALUE|failureSuccess|' + results[0].success
        );
        System.debug(
            'APEX_EXEC_LOCAL_VALUE|failureMessage|' +
            results[0].errors[0].message
        );
    }
}
