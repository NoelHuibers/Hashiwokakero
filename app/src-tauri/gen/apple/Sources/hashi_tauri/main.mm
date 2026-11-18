#include "bindings/bindings.h"

#import <UIKit/UIKit.h>
#import <UserNotifications/UserNotifications.h>
#import <objc/runtime.h>

// Notifications through Apple's push service (docs/notifications.md, "The iOS app"). A tap on
// one opens its `url` (a page of https://hashi.rs) in the app as a Universal Link would: through
// the scene delegate's scene:continueUserActivity:, which tao passes on to the deep-link plugin,
// whose page shows it (api::native::follow_app_links). A tap that starts the app may come before
// its scene is there: the link waits for the scene (passOnLaunchLinks).
static NSURL *pendingLink;
// The tapped notification last opened (its request and date): iOS may hand the one that started
// the app to the delegate and in the scene's connection options both
static NSString *openedNotification;

static void openLink(UIScene *scene, NSURL *link) {
	NSUserActivity *activity = [[NSUserActivity alloc] initWithActivityType:NSUserActivityTypeBrowsingWeb];
	activity.webpageURL = link;
	[(id<UISceneDelegate>)scene.delegate scene:scene continueUserActivity:activity];
}

static void openInApp(NSURL *link) {
	for (UIScene *scene in UIApplication.sharedApplication.connectedScenes) {
		if (scene.activationState != UISceneActivationStateUnattached
			&& [scene.delegate respondsToSelector:@selector(scene:continueUserActivity:)]) {
			openLink(scene, link);
			return;
		}
	}
	pendingLink = link;
}

static void openTapped(UNNotificationResponse *response) {
	if (![response.actionIdentifier isEqualToString:UNNotificationDefaultActionIdentifier]) {
		return;
	}
	UNNotification *notification = response.notification;
	NSString *seen = [NSString stringWithFormat:@"%@ %f", notification.request.identifier, notification.date.timeIntervalSince1970];
	if ([seen isEqualToString:openedNotification]) {
		return;
	}
	openedNotification = seen;
	id url = notification.request.content.userInfo[@"url"];
	NSURL *link = [url isKindOfClass:[NSString class]] ? [NSURL URLWithString:url] : nil;
	if ([link.scheme isEqualToString:@"https"]) {
		openInApp(link);
	}
}

// The notification centre's delegate: a notification that comes while the app is in front shows
// as a banner as well, and a tap opens its page
@interface HashiNotifications : NSObject <UNUserNotificationCenterDelegate>
@end

@implementation HashiNotifications
- (void)userNotificationCenter:(UNUserNotificationCenter *)center willPresentNotification:(UNNotification *)notification withCompletionHandler:(void (^)(UNNotificationPresentationOptions))completionHandler {
	completionHandler(UNNotificationPresentationOptionBanner | UNNotificationPresentationOptionList | UNNotificationPresentationOptionSound);
}

- (void)userNotificationCenter:(UNUserNotificationCenter *)center didReceiveNotificationResponse:(UNNotificationResponse *)response withCompletionHandler:(void (^)(void))completionHandler {
	openTapped(response);
	completionHandler();
}
@end

// iOS hands the device token for Apple's push service to the application's delegate, tao's,
// which has no such methods: they are added to its class before UIKit takes the delegate (it
// notes what a delegate answers to then) and pass the token on to the app's plugin
// (HashiPlugin.swift, notify_token) as a notification of NSNotificationCenter.
static void addTokenCallbacks(Class delegateClass) {
	class_addMethod(delegateClass, @selector(application:didRegisterForRemoteNotificationsWithDeviceToken:), imp_implementationWithBlock(^(id delegate, UIApplication *application, NSData *token) {
		[NSNotificationCenter.defaultCenter postNotificationName:@"HashiDeviceToken" object:token];
	}), "v@:@@");
	class_addMethod(delegateClass, @selector(application:didFailToRegisterForRemoteNotificationsWithError:), imp_implementationWithBlock(^(id delegate, UIApplication *application, NSError *error) {
		[NSNotificationCenter.defaultCenter postNotificationName:@"HashiDeviceTokenFailed" object:error];
	}), "v@:@@");
}

// The app runs on the scene lifecycle (UIApplicationSceneManifest in Info.plist, for the safe
// areas in landscape). There iOS hands the links the app is started with (Universal Links) to
// the scene delegate's scene:willConnectToSession:options:, which tao's TaoSceneDelegate ignores
// (tao 0.37): it passes on only the links of a running app, scene:continueUserActivity: and
// scene:openURLContexts:, to the deep-link plugin. After tao's own method, which starts the app
// (Tauri's setup and its plugins), the launch's links go to those two as well.
static void passOnLaunchLinks(Class delegateClass) {
	static BOOL done = NO;
	SEL willConnect = @selector(scene:willConnectToSession:options:);
	Method method = class_getInstanceMethod(delegateClass, willConnect);
	if (done || !method) {
		return;
	}
	done = YES;
	typedef void (*WillConnect)(id, SEL, UIScene *, UISceneSession *, UISceneConnectionOptions *);
	WillConnect original = (WillConnect)method_getImplementation(method);
	method_setImplementation(method, imp_implementationWithBlock(^(id<UISceneDelegate> delegate, UIScene *scene, UISceneSession *session, UISceneConnectionOptions *options) {
		original(delegate, willConnect, scene, session, options);
		for (NSUserActivity *activity in options.userActivities) {
			if (activity.webpageURL && [delegate respondsToSelector:@selector(scene:continueUserActivity:)]) {
				[delegate scene:scene continueUserActivity:activity];
			}
		}
		if (options.URLContexts.count && [delegate respondsToSelector:@selector(scene:openURLContexts:)]) {
			[delegate scene:scene openURLContexts:options.URLContexts];
		}
		// The notification the app was started with, by a tap
		if (options.notificationResponse) {
			openTapped(options.notificationResponse);
		}
		if (pendingLink && [delegate respondsToSelector:@selector(scene:continueUserActivity:)]) {
			NSURL *link = pendingLink;
			pendingLink = nil;
			openLink(scene, link);
		}
	}));
}

// tao registers its scene delegate's class before UIKit starts, and names it when UIKit asks the
// app for a new scene's configuration. A scene restored from an earlier launch (the usual case
// after the first one) isn't asked for: UIKit looks the class up by name. So the method is
// wrapped once UIKit has given the application its delegate, before any scene connects, and when
// the class is named, whichever comes first.
static void replace(Class cls, SEL selector, id block, IMP *original) {
	Method method = class_getInstanceMethod(cls, selector);
	*original = method_getImplementation(method);
	method_setImplementation(method, imp_implementationWithBlock(block));
}

static void watchSceneDelegates(void) {
	static IMP setDelegate, setDelegateClass;
	static HashiNotifications *notifications;
	replace([UIApplication class], @selector(setDelegate:), ^(UIApplication *application, id delegate) {
		// Before UIKit has it, and before the app finished launching: a tap that starts the app
		// goes to the notification centre's delegate then
		if (delegate) {
			addTokenCallbacks([delegate class]);
			notifications = [HashiNotifications new];
			UNUserNotificationCenter.currentNotificationCenter.delegate = notifications;
		}
		((void (*)(id, SEL, id))setDelegate)(application, @selector(setDelegate:), delegate);
		Class delegateClass = NSClassFromString(@"TaoSceneDelegate");
		if (delegateClass) {
			passOnLaunchLinks(delegateClass);
		}
	}, &setDelegate);
	replace([UISceneConfiguration class], @selector(setDelegateClass:), ^(UISceneConfiguration *configuration, Class delegateClass) {
		if (delegateClass) {
			passOnLaunchLinks(delegateClass);
		}
		((void (*)(id, SEL, Class))setDelegateClass)(configuration, @selector(setDelegateClass:), delegateClass);
	}, &setDelegateClass);
}

int main(int argc, char * argv[]) {
	watchSceneDelegates();
	ffi::start_app();
	return 0;
}
