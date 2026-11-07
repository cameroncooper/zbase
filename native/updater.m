// All entry points and Sparkle callbacks run on AppKit's main thread.
#import <AppKit/AppKit.h>
#import <Sparkle/Sparkle.h>

@interface ZBaseUpdateDriver : NSObject <SPUUserDriver, SPUUpdaterDelegate>
@property SPUUpdater *updater;
@property NSMutableDictionary *snapshot;
@property SUAppcastItem *item;
@property(copy) void (^choice)(SPUUserUpdateChoice);
@property(copy) void (^cancel)(void);
@property BOOL manual;
@property BOOL terminal;
@property BOOL installAfterCheck;
@property(copy) void (^automaticInstall)(void);
@end
static ZBaseUpdateDriver *driver;

@implementation ZBaseUpdateDriver
- (instancetype)init {
    if ((self = [super init])) {
        _snapshot = [@{@"state": @"idle", @"version": @"", @"notes": @"", @"release_url": @"", @"message": @"", @"received": @0, @"total": @0, @"attention": @NO} mutableCopy];
    }
    return self;
}
- (void)setStage:(NSString *)stage { self.snapshot[@"state"] = stage; }
- (void)showUpdatePermissionRequest:(SPUUpdatePermissionRequest *)request reply:(void (^)(SUUpdatePermissionResponse *))reply {
    (void)request;
    // Official bundles explicitly enable checks in Info.plist. No profile data.
    reply([[SUUpdatePermissionResponse alloc] initWithAutomaticUpdateChecks:YES sendSystemProfile:NO]);
}
- (void)showUserInitiatedUpdateCheckWithCancellation:(void (^)(void))cancellation {
    self.manual = YES; self.terminal = NO; self.cancel = cancellation;
    self.snapshot[@"attention"] = @YES;
    [self setStage:@"checking"];
}
- (void)updater:(SPUUpdater *)updater didFindValidUpdate:(SUAppcastItem *)item {
    (void)updater;
    self.item = item;
    self.snapshot[@"version"] = item.displayVersionString ?: @"";
    self.snapshot[@"notes"] = item.itemDescription ?: @"";
    self.snapshot[@"release_url"] = item.infoURL.absoluteString ?: @"https://github.com/cameroncooper/zbase/releases";
}
- (void)showUpdateFoundWithAppcastItem:(SUAppcastItem *)item state:(SPUUserUpdateState *)state reply:(void (^)(SPUUserUpdateChoice))reply {
    self.item = item; self.choice = reply; self.cancel = nil; self.terminal = NO;
    self.manual = state.userInitiated;
    self.snapshot[@"attention"] = @(state.userInitiated);
    self.snapshot[@"version"] = item.displayVersionString ?: @"";
    self.snapshot[@"notes"] = item.itemDescription ?: @"";
    self.snapshot[@"release_url"] = item.infoURL.absoluteString ?: @"https://github.com/cameroncooper/zbase/releases";
    self.snapshot[@"message"] = @"";
    self.snapshot[@"informational"] = @(item.informationOnlyUpdate);
    [self setStage:state.stage == SPUUserUpdateStageInstalling ? @"ready" : (state.stage == SPUUserUpdateStageDownloaded ? @"downloaded" : @"available")];
    BOOL shouldInstall = self.installAfterCheck;
    self.installAfterCheck = NO;
    if (shouldInstall && !item.informationOnlyUpdate) {
        self.choice = nil;
        reply(SPUUserUpdateChoiceInstall);
    }
}
- (void)showUpdateReleaseNotesWithDownloadData:(SPUDownloadData *)data {
    // Feed notes are UTF-8 plain text. Never execute remote HTML in the app.
    self.snapshot[@"notes"] = [[NSString alloc] initWithData:data.data encoding:NSUTF8StringEncoding] ?: @"Release notes are available on GitHub.";
}
- (void)showUpdateReleaseNotesFailedToDownloadWithError:(NSError *)error {
    (void)error; self.snapshot[@"notes"] = @"Release notes are available on GitHub.";
}
- (void)showUpdateNotFoundWithError:(NSError *)error acknowledgement:(void (^)(void))acknowledgement {
    self.terminal = YES; self.cancel = nil; self.installAfterCheck = NO;
    // Sparkle's explanation also covers incompatible OS / newer development versions.
    self.snapshot[@"message"] = error.localizedDescription ?: @"You’re up to date.";
    [self setStage:@"current"]; acknowledgement();
}
- (void)showUpdaterError:(NSError *)error acknowledgement:(void (^)(void))acknowledgement {
    self.terminal = YES; self.choice = nil; self.cancel = nil; self.installAfterCheck = NO; self.automaticInstall = nil;
    self.snapshot[@"message"] = error.localizedDescription ?: @"Couldn’t finish the update. Try again later.";
    [self setStage:@"error"]; acknowledgement();
}
- (void)showDownloadInitiatedWithCancellation:(void (^)(void))cancellation {
    self.cancel = cancellation; self.choice = nil; self.terminal = NO;
    self.snapshot[@"received"] = @0; self.snapshot[@"total"] = @0;
    [self setStage:@"downloading"];
}
- (void)showDownloadDidReceiveExpectedContentLength:(uint64_t)length { self.snapshot[@"total"] = @(length); }
- (void)showDownloadDidReceiveDataOfLength:(uint64_t)length {
    self.snapshot[@"received"] = @([self.snapshot[@"received"] unsignedLongLongValue] + length);
}
- (void)showDownloadDidStartExtractingUpdate {
    self.cancel = nil; [self setStage:@"preparing"];
    self.snapshot[@"message"] = @"macOS may ask permission to update this application.";
}
- (void)showExtractionReceivedProgress:(double)progress { (void)progress; }
- (void)showReadyToInstallAndRelaunch:(void (^)(SPUUserUpdateChoice))reply {
    self.choice = reply; self.cancel = nil; self.terminal = NO; [self setStage:@"ready"];
}
- (void)showInstallingUpdateWithApplicationTerminated:(BOOL)terminated retryTerminatingApplication:(void (^)(void))retry {
    (void)terminated; (void)retry; [self setStage:@"installing"];
}
- (void)showUpdateInstalledAndRelaunched:(BOOL)relaunched acknowledgement:(void (^)(void))acknowledgement {
    (void)relaunched; self.terminal = YES; [self setStage:@"current"]; acknowledgement();
}
- (void)dismissUpdateInstallation {
    self.choice = nil; self.cancel = nil; self.automaticInstall = nil;
    if (!self.terminal) [self setStage:@"idle"];
}
- (BOOL)updater:(SPUUpdater *)updater willInstallUpdateOnQuit:(SUAppcastItem *)item immediateInstallationBlock:(void (^)(void))install {
    [self updater:updater didFindValidUpdate:item];
    self.automaticInstall = install;
    self.terminal = NO;
    __weak ZBaseUpdateDriver *weakSelf = self;
    self.choice = ^(SPUUserUpdateChoice choice) {
        if (choice == SPUUserUpdateChoiceInstall) {
            void (^handler)(void) = weakSelf.automaticInstall;
            weakSelf.automaticInstall = nil;
            if (handler) handler();
        }
    };
    [self setStage:@"ready"];
    return YES;
}
- (void)showUpdateInFocus { self.snapshot[@"attention"] = @YES; }
- (void)updater:(SPUUpdater *)updater didFinishUpdateCycleForUpdateCheck:(SPUUpdateCheck)check error:(NSError *)error {
    (void)updater; (void)check;
    // Scheduled failures do not open UI or produce an error badge.
    if (error && !self.manual && !self.terminal && !self.choice) [self setStage:@"idle"];
    self.manual = NO;
}
@end

void zbase_update_start(void) {
    NSCAssert([NSThread isMainThread], @"Updater must run on main thread");
    if (driver) return;
    NSBundle *bundle = NSBundle.mainBundle;
    if (![[bundle objectForInfoDictionaryKey:@"ZBaseDistribution"] isEqual:@"official"]) return;
    driver = [[ZBaseUpdateDriver alloc] init];
    NSNumber *readOnly = nil;
    [bundle.bundleURL getResourceValue:&readOnly forKey:NSURLVolumeIsReadOnlyKey error:nil];
    if (readOnly.boolValue || [bundle.bundlePath containsString:@"/Cellar/"]) {
        driver.snapshot[@"state"] = @"disabled";
        driver.snapshot[@"message"] = readOnly.boolValue ? @"Move zBase to Applications and reopen it to enable updates." : @"This installation is managed by Homebrew. Use brew upgrade zbase.";
        return;
    }
    driver.updater = [[SPUUpdater alloc] initWithHostBundle:bundle applicationBundle:bundle userDriver:driver delegate:driver];
    NSError *error = nil;
    if (![driver.updater startUpdater:&error]) {
        driver.terminal = YES; [driver setStage:@"error"];
        driver.snapshot[@"message"] = error.localizedDescription ?: @"Updater is unavailable.";
    }
}
char *zbase_update_snapshot(void) {
    if (!driver) return NULL;
    driver.snapshot[@"automatic_checks"] = @(driver.updater.automaticallyChecksForUpdates);
    driver.snapshot[@"automatic_downloads"] = @(driver.updater.automaticallyDownloadsUpdates);
    driver.snapshot[@"last_check"] = @([driver.updater.lastUpdateCheckDate timeIntervalSince1970]);
    NSData *data = [NSJSONSerialization dataWithJSONObject:driver.snapshot options:0 error:nil];
    driver.snapshot[@"attention"] = @NO;
    return strdup([[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding].UTF8String);
}
void zbase_update_action(int action) {
    if (!driver) return;
    switch (action) {
        case 0:
            driver.installAfterCheck = NO;
            if (driver.updater.canCheckForUpdates) { driver.terminal = NO; [driver.updater checkForUpdates]; }
            else driver.snapshot[@"attention"] = @YES;
            break;
        case 1: case 5: {
            if (driver.item.informationOnlyUpdate) { [NSWorkspace.sharedWorkspace openURL:driver.item.infoURL]; break; }
            void (^reply)(SPUUserUpdateChoice) = driver.choice;
            driver.choice = nil;
            if (reply) reply(SPUUserUpdateChoiceInstall);
            else if (driver.updater.canCheckForUpdates) {
                // Later ended the previous cycle. Revalidate the feed before acting
                // on a cached reminder; Sparkle can resume a prepared installation.
                driver.installAfterCheck = YES; driver.terminal = NO;
                [driver.updater checkForUpdates];
            }
            break;
        }
        case 2: {
            driver.snapshot[@"attention"] = @NO;
            if (driver.automaticInstall) break; // A prepared background install keeps its explicit restart handler.
            void (^reply)(SPUUserUpdateChoice) = driver.choice; driver.choice = nil;
            if (reply) {
                // Keep a quiet reminder but release Sparkle's session so future
                // scheduled checks can discover a newer release.
                driver.terminal = YES;
                reply(SPUUserUpdateChoiceDismiss);
            }
            break;
        }
        case 3: {
            void (^reply)(SPUUserUpdateChoice) = driver.choice; driver.choice = nil;
            if (reply) reply(SPUUserUpdateChoiceSkip);
            break;
        }
        case 4: {
            driver.installAfterCheck = NO;
            void (^cancel)(void) = driver.cancel; driver.cancel = nil;
            if (cancel) cancel();
            [driver setStage:@"idle"];
            break;
        }
        case 6: driver.updater.automaticallyChecksForUpdates = !driver.updater.automaticallyChecksForUpdates; break;
        case 7: driver.updater.automaticallyDownloadsUpdates = !driver.updater.automaticallyDownloadsUpdates; break;
    }
}
