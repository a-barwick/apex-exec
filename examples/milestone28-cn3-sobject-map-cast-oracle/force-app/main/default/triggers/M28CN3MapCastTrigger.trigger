trigger M28CN3MapCastTrigger on Account (after update) {
    Map<Id, Account> typed = (Map<Id, Account>) Trigger.oldMap;
    System.debug('APEX_EXEC_ORACLE_VALUE|triggerTyped|' +
        (typed.get(Trigger.new[0].Id).Name == 'before'));
}
