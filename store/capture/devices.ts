// The screens the pages are captured on, and the store images framed from them.

/**
 * A screen the site is captured on, like the app shows it: edge to edge, behind the system bars
 * (the phones), or the desktop app's window.
 */
export interface Screen {
  id: 'phone' | 'tablet' | 'desktop';
  /** CSS pixels, the whole screen. */
  width: number;
  height: number;
  scale: number;
  /** What the system bars cover, as the app's --app-inset-* (app/README.md, On the phone). */
  insetTop: number;
  insetBottom: number;
}

export const SCREENS: Record<Screen['id'], Screen> = {
  // An iPhone 16 Pro: 1206 × 2622
  phone: { id: 'phone', width: 402, height: 874, scale: 3, insetTop: 54, insetBottom: 30 },
  // An iPad Pro 13": 2064 × 2752
  tablet: { id: 'tablet', width: 1032, height: 1376, scale: 2, insetTop: 28, insetBottom: 20 },
  // The desktop app's window on a laptop (2560 × 1600): the website's desktop layout (from
  // 1024 px), no system bars over the page (the template draws the title bar above it)
  desktop: { id: 'desktop', width: 1280, height: 800, scale: 2, insetTop: 0, insetBottom: 0 },
};

/** A store image: its size, the screen in it, and the store's folder for it. */
export interface Frame {
  id: string;
  /** Whose: the App Store's, Google Play's, or the desktop stores' (Microsoft Store, Steam, itch.io). */
  store: 'apple' | 'play' | 'desktop';
  width: number;
  height: number;
  screen: Screen['id'];
  /** fastlane's folder: deliver sorts by size, supply by folder name. */
  folder: string;
  /** The caption: its distance from the top, the hanko, the title and the text, in % of the width. */
  sizes: { top: number; hanko: number; title: number; text: number };
}

const PHONE_TALL = { top: 9, hanko: 9.5, title: 7.4, text: 3.9 };
const PORTRAIT = { top: 6.5, hanko: 8.5, title: 7, text: 3.6 };
const TABLET = { top: 6, hanko: 6.5, title: 5.2, text: 2.7 };
const LANDSCAPE = { top: 2.6, hanko: 3.8, title: 3.1, text: 1.55 };

export const FRAMES: Frame[] = [
  // App Store: iPhone 6.9" and iPad 13" (App Store Connect scales them for the smaller ones)
  {
    id: 'iphone-69',
    store: 'apple',
    width: 1320,
    height: 2868,
    screen: 'phone',
    folder: 'iphone69',
    sizes: PHONE_TALL,
  },
  {
    id: 'ipad-13',
    store: 'apple',
    width: 2064,
    height: 2752,
    screen: 'tablet',
    folder: 'ipad13',
    sizes: TABLET,
  },
  // Google Play: 9:16 portrait, the phone and both tablet sizes
  {
    id: 'play-phone',
    store: 'play',
    width: 1080,
    height: 1920,
    screen: 'phone',
    folder: 'phoneScreenshots',
    sizes: PORTRAIT,
  },
  {
    id: 'play-tablet-7',
    store: 'play',
    width: 1080,
    height: 1920,
    screen: 'tablet',
    folder: 'sevenInchScreenshots',
    sizes: PORTRAIT,
  },
  {
    id: 'play-tablet-10',
    store: 'play',
    width: 1440,
    height: 2560,
    screen: 'tablet',
    folder: 'tenInchScreenshots',
    sizes: PORTRAIT,
  },
  // The desktop stores: 16:9, 1920 × 1080, the window of the desktop app. The Microsoft Store
  // takes 1366 × 768 or larger; Steam wants 16:9 at 1920 × 1080 (step 10 of docs/stores.md).
  {
    id: 'desktop',
    store: 'desktop',
    width: 1920,
    height: 1080,
    screen: 'desktop',
    folder: 'desktop',
    sizes: LANDSCAPE,
  },
];
