public class M28CN3MapCastOracle {
    public static void run() {
        Account account = new Account(Id = '001000000000001AAA', Name = 'typed');
        Map<Id, SObject> source = new Map<Id, SObject>{ account.Id => account };
        try {
            Map<Id, Account> rejected = (Map<Id, Account>) source;
            System.debug('APEX_EXEC_ORACLE_VALUE|rejected|false');
        } catch (TypeException error) {
            System.debug('APEX_EXEC_ORACLE_VALUE|rejected|true');
        }

        Account inserted = new Account(Name = 'before');
        insert inserted;
        inserted.Name = 'after';
        update inserted;
        delete inserted;
    }
}
