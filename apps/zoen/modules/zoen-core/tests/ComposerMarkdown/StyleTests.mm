#import "ENRMInputFormatter.h"
#import "ENRMEditSession.h"

@interface MarkedTextView : NSTextView
@property BOOL composing;
@end
@implementation MarkedTextView
- (BOOL)hasMarkedText { return self.composing; }
@end

// Only the host's unrelated integration collaborators are test doubles.
// Formatting, font resolution, edit-phase guards and attributed layout are real.
@interface Store : NSObject
@property(copy) NSArray *allRanges;
- (void)setRanges:(NSArray *)ranges;
@end
@implementation Store
- (void)setRanges:(NSArray *)ranges { self.allRanges = ranges; }
@end
@interface Composer : NSObject
@property(copy) NSString *binding;
@end
@implementation Composer
@end
static NSArray *ENRMNormalizeComposerRanges(NSString *text, NSArray *ranges) {
  NSCAssert(NO, @"range serialization has its own production suite");
  return ranges;
}
@interface Detector : NSObject
- (void)refreshAllStyling;
@end
@implementation Detector
- (void)refreshAllStyling {}
@end

@interface Input : NSObject {
@public
  MarkedTextView *_textView;
  NSTextField *_placeholderLabel;
  ENRMInputFormatter *_formatter;
  ENRMInputFormatterStyle *_formatterStyle;
  ENRMEditSession *_editSession;
  Store *_formattingStore, *_blockStore;
  Composer *_composer;
  Detector *_detectorPipeline;
  BOOL _draftSealed, _draftDiscardFrozen, _needsFullStyleRefresh;
  NSUInteger _heightUpdates;
}
- (void)applyFormatting;
- (void)applyFormattingScopedToRange:(NSRange)scope;
- (void)resetBaseTypingAttributes;
- (void)styleChanged;
- (void)requestHeightUpdate;
- (void)applyWritingDirection;
@end
@implementation Input
- (instancetype)init {
  if ((self = [super init])) {
    _textView = [[MarkedTextView alloc] initWithFrame:NSMakeRect(0, 0, 240, 200)];
    _placeholderLabel = [NSTextField new];
    _formatter = [ENRMInputFormatter new];
    _formatterStyle = [ENRMInputFormatterStyle new];
    _editSession = [[ENRMEditSession alloc] initWithTextView:_textView];
    _formattingStore = [Store new]; _formattingStore.allRanges = @[];
    _blockStore = [Store new]; _blockStore.allRanges = @[];
    _detectorPipeline = [Detector new];
  }
  return self;
}
- (void)requestHeightUpdate { _heightUpdates++; }
- (void)applyWritingDirection {}
- (void)styleChanged {
#include "StyleUpdate.inc"
}
#include "StyleMethods.inc"
@end

static void expectFont(Input *input, CGFloat size, NSRange selection) {
  NSString *text = @"café 🪷\nsecond line\n";
  NSCAssert([input->_textView.string isEqual:text], @"style changes preserve every character");
  NSCAssert(NSEqualRanges(input->_textView.selectedRange, selection), @"style changes preserve selection");
  NSCAssert(!input->_needsFullStyleRefresh, @"stable refresh consumes invalidation");
  [input->_textView.textStorage enumerateAttribute:NSFontAttributeName
    inRange:NSMakeRange(0, text.length) options:0 usingBlock:^(NSFont *font, NSRange range, BOOL *stop) {
      NSCAssert(fabs(font.pointSize - size) < 0.01, @"all existing runs use the new size");
    }];
}

int main(void) {
  @autoreleasepool {
    [NSApplication sharedApplication];
    Input *input = [Input new];
    NSString *text = @"café 🪷\nsecond line\n";
    [input->_textView.textStorage setAttributedString:[[NSAttributedString alloc]
      initWithString:text attributes:@{NSFontAttributeName: [NSFont systemFontOfSize:17]}]];
    NSRange selection = NSMakeRange(2, 3);
    input->_textView.selectedRange = selection;
    for (NSNumber *size in @[@17, @51, @61, @17]) {
      input->_formatterStyle.baseFont = [NSFont systemFontOfSize:size.doubleValue];
      [input styleChanged];
      expectFont(input, size.doubleValue, selection);
    }
    NSCAssert(input->_heightUpdates == 4, @"each style update requests fitting");

    input->_formatterStyle.baseTextColor = NSColor.orangeColor;
    [input styleChanged];
    [input->_textView.textStorage enumerateAttribute:NSForegroundColorAttributeName
      inRange:NSMakeRange(0, text.length) options:0 usingBlock:^(NSColor *color, NSRange range, BOOL *stop) {
        NSCAssert([color isEqual:NSColor.orangeColor], @"plain text follows live theme changes");
      }];

    input->_formattingStore.allRanges = @[
      [ENRMFormattingRange rangeWithType:ENRMInputStyleTypeStrong range:NSMakeRange(0, 4)],
      [ENRMFormattingRange rangeWithType:ENRMInputStyleTypeCode range:NSMakeRange(8, 6)],
    ];
    input->_formatterStyle.baseFont = [NSFont systemFontOfSize:51];
    [input styleChanged];
    expectFont(input, 51, selection);
    NSFont *bold = [input->_textView.textStorage attribute:NSFontAttributeName atIndex:0 effectiveRange:NULL];
    NSFont *code = [input->_textView.textStorage attribute:NSFontAttributeName atIndex:8 effectiveRange:NULL];
    NSCAssert(bold.fontDescriptor.symbolicTraits & NSFontDescriptorTraitBold, @"bold survives restyling");
    NSCAssert(code.fontDescriptor.symbolicTraits & NSFontDescriptorTraitMonoSpace, @"code face survives restyling");

    input->_textView.composing = YES;
    NSAttributedString *before = [input->_textView.textStorage copy];
    input->_formatterStyle.baseFont = [NSFont systemFontOfSize:17];
    [input styleChanged];
    NSCAssert([input->_textView.textStorage isEqual:before], @"IME content is not restyled mid-composition");
    NSCAssert(input->_needsFullStyleRefresh, @"full refresh waits for composition");
    input->_textView.composing = NO;
    // A commit may only request the touched paragraph. The pending update must
    // expand it to the whole document, including unchanged earlier paragraphs.
    [input applyFormattingScopedToRange:NSMakeRange(8, 6)];
    expectFont(input, 17, selection);

    [input->_editSession enterPhase:ENRMEditPhaseImporting];
    input->_formatterStyle.baseFont = [NSFont systemFontOfSize:51];
    [input styleChanged];
    NSCAssert(input->_needsFullStyleRefresh, @"import suppression retains pending refresh");
    [input->_editSession exitPhase];
    input->_draftSealed = YES;
    [input applyFormatting];
    NSCAssert(input->_needsFullStyleRefresh, @"sealed draft remains immutable");
    input->_draftSealed = NO; input->_draftDiscardFrozen = YES;
    [input applyFormatting];
    NSCAssert(input->_needsFullStyleRefresh, @"discard freeze remains immutable");
    input->_draftDiscardFrozen = NO;
    [input applyFormattingScopedToRange:NSMakeRange(8, 6)];
    expectFont(input, 51, selection);

    // Once consumed, a normal scoped edit must not reformat untouched text.
    NSFont *sentinel = [NSFont systemFontOfSize:23];
    [input->_textView.textStorage addAttribute:NSFontAttributeName value:sentinel range:NSMakeRange(0, 4)];
    [input applyFormattingScopedToRange:NSMakeRange(8, 6)];
    NSCAssert([input->_textView.textStorage attribute:NSFontAttributeName atIndex:0 effectiveRange:NULL] == sentinel,
      @"normal edits retain the upstream scoped formatting optimization");
    puts("Maintained composer style PASS");
  }
}
