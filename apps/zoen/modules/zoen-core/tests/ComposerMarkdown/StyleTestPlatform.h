#pragma once
#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>

// Test-only platform aliases. The maintained formatter and style handlers run
// unchanged against AppKit's real attributed storage, fonts and layout manager.
#define UIFont NSFont
#define UIFontDescriptor NSFontDescriptor
#define UIFontDescriptorSymbolicTraits NSFontDescriptorSymbolicTraits
#define UIFontDescriptorTraitBold NSFontDescriptorTraitBold
#define UIFontDescriptorTraitItalic NSFontDescriptorTraitItalic
#define UIFontDescriptorTraitsAttribute NSFontTraitsAttribute
#define UIFontWeightTrait NSFontWeightTrait
#define UIFontWeight NSFontWeight
#define UIFontWeightRegular NSFontWeightRegular
#define UIFontWeightUltraLight NSFontWeightUltraLight
#define UIFontWeightThin NSFontWeightThin
#define UIFontWeightLight NSFontWeightLight
#define UIFontWeightMedium NSFontWeightMedium
#define UIFontWeightSemibold NSFontWeightSemibold
#define UIFontWeightBold NSFontWeightBold
#define UIFontWeightHeavy NSFontWeightHeavy
#define UIFontWeightBlack NSFontWeightBlack
#define RCTUIColor NSColor
#define ENRMPlatformTextView NSTextView
static inline NSString *ENRMGetPlainText(NSTextView *view) { return view.string; }
static inline BOOL ENRMHasMarkedText(NSTextView *view) { return view.hasMarkedText; }
static inline void ENRMSetNeedsDisplay(NSTextView *view) { view.needsDisplay = YES; }
static inline void ENRMSetDefaultTypingAttributes(NSTextView *view, NSDictionary *attributes) {
  view.typingAttributes = attributes;
}
