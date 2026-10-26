#include "bindings/bindings.h"

#import <UIKit/UIKit.h>
#import <objc/runtime.h>

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
	replace([UIApplication class], @selector(setDelegate:), ^(UIApplication *application, id delegate) {
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
