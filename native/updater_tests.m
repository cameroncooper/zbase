// Exercise the custom driver without touching a real app or a public feed.
#import "updater.m"
@interface TestItem : NSObject
@property NSString *displayVersionString;
@property NSString *itemDescription;
@property NSURL *infoURL;
@property(getter=isInformationOnlyUpdate) BOOL informationOnlyUpdate;
@end
@implementation TestItem
@end
@interface TestState : NSObject
@property BOOL userInitiated;
@property SPUUserUpdateStage stage;
@end
@implementation TestState
@end
int main(void) {
    @autoreleasepool {
        zbase_update_start();
        NSCAssert(zbase_update_snapshot() == NULL, @"Unmarked development bundles cannot update");
        driver = [[ZBaseUpdateDriver alloc] init];
        TestItem *item = [TestItem new]; item.displayVersionString = @"1.0.15"; item.itemDescription = @"Notes";
        TestState *state = [TestState new];
        __block NSInteger choices = 0, dismissals = 0;
        void (^foundReply)(SPUUserUpdateChoice) = ^(SPUUserUpdateChoice choice) {
            if (choice == SPUUserUpdateChoiceDismiss) dismissals++;
            else { NSCAssert(choice == SPUUserUpdateChoiceInstall, @"Install choice"); choices++; }
        };
        [driver showUpdateFoundWithAppcastItem:(id)item state:(id)state reply:foundReply];
        NSCAssert([driver.snapshot[@"state"] isEqual:@"available"], @"Found update");
        zbase_update_action(2);
        NSCAssert(driver.choice == nil && choices == 0 && dismissals == 1, @"Later must end the update cycle without installing");
        [driver dismissUpdateInstallation];
        NSCAssert([driver.snapshot[@"state"] isEqual:@"available"], @"Keep the quiet reminder");
        [driver showUpdateFoundWithAppcastItem:(id)item state:(id)state reply:foundReply];
        zbase_update_action(1); zbase_update_action(1);
        NSCAssert(choices == 1, @"Callbacks must only be consumed once");
        [driver showDownloadInitiatedWithCancellation:^{ choices++; }];
        [driver showDownloadDidReceiveExpectedContentLength:100];
        [driver showDownloadDidReceiveDataOfLength:40]; [driver showDownloadDidReceiveDataOfLength:20];
        NSCAssert([driver.snapshot[@"received"] integerValue] == 60, @"Accumulate deltas");
        zbase_update_action(4); zbase_update_action(4);
        NSCAssert(choices == 2, @"Cancel must only be consumed once");
        [driver showReadyToInstallAndRelaunch:foundReply];
        zbase_update_action(2);
        NSCAssert(choices == 2 && [driver.snapshot[@"state"] isEqual:@"ready"], @"Hiding cannot restart");
        NSCAssert(dismissals == 2, @"Ready dismissal releases the session without restarting");
        [driver showReadyToInstallAndRelaunch:foundReply];
        zbase_update_action(5);
        NSCAssert(choices == 3, @"Explicit restart resumes installation");
        [driver showUpdaterError:[NSError errorWithDomain:@"test" code:1 userInfo:@{NSLocalizedDescriptionKey:@"Download failed"}] acknowledgement:^{}];
        [driver dismissUpdateInstallation];
        NSCAssert([driver.snapshot[@"state"] isEqual:@"error"], @"Terminal error survives dismissal");
        __block NSInteger automaticInstalls = 0;
        NSCAssert([driver updater:(id)[NSObject new] willInstallUpdateOnQuit:(id)item immediateInstallationBlock:^{ automaticInstalls++; }], @"Custom UI owns prepared background installs");
        NSCAssert([driver.snapshot[@"state"] isEqual:@"ready"], @"Background download exposes the restart action");
        zbase_update_action(2);
        NSCAssert(automaticInstalls == 0, @"Later cannot relaunch an automatic update");
        zbase_update_action(5); zbase_update_action(5);
        NSCAssert(automaticInstalls == 1, @"Explicit automatic installation is consumed once");
        item.informationOnlyUpdate = YES;
        driver.installAfterCheck = YES;
        [driver showUpdateFoundWithAppcastItem:(id)item state:(id)state reply:foundReply];
        NSCAssert(!driver.installAfterCheck, @"An informational result cannot leave a latent install request");
        puts("Updater bridge tests passed");
    }
    return 0;
}
