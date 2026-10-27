#import <Foundation/Foundation.h>
#import "ENRMInputParser.h"
#import "ENRMMarkdownSerializer.h"
#import "ENRMFormattingStore.h"
#import "ENRMBlockStore.h"

int main() { @autoreleasepool {
  NSError *error=nil;
  NSData *input=[[NSFileHandle fileHandleWithStandardInput] readDataToEndOfFile];
  NSDictionary *request=[NSJSONSerialization JSONObjectWithData:input options:0 error:&error];
  if(error || ![request[@"markdown"] isKindOfClass:NSString.class])return 2;
  BOOL generic=[request[@"generic"] boolValue];
  ENRMInputParser *parser=[ENRMInputParser new];
  ENRMParseResult *parsed=generic ? [parser parseToPlainTextAndRanges:request[@"markdown"]] : [parser parseComposerMarkdown:request[@"markdown"]];
  NSMutableString *text=[parsed.plainText mutableCopy];
  ENRMFormattingStore *styles=[ENRMFormattingStore new];[styles setRanges:parsed.formattingRanges];
  ENRMBlockStore *blocks=[ENRMBlockStore new];[blocks setRanges:parsed.blockRanges];
  NSDictionary *edit=request[@"edit"];
  if(edit){NSUInteger start=[edit[@"start"] unsignedIntegerValue],length=[edit[@"delete"] unsignedIntegerValue];
    NSString *value=edit[@"text"];
    if(start>text.length || length>text.length-start || ![value isKindOfClass:NSString.class])return 3;
    [text replaceCharactersInRange:NSMakeRange(start,length) withString:value];
    [styles adjustForEditAtLocation:start deletedLength:length insertedLength:value.length];
    [blocks adjustForEditAtLocation:start deletedLength:length insertedLength:value.length];
    [blocks normalizeToLineBoundsInText:text];
  }
  NSArray *code=request[@"code"];
  if(code)[styles toggleStyle:ENRMInputStyleTypeCode inRange:NSMakeRange([code[0] unsignedIntegerValue],[code[1] unsignedIntegerValue]) conflictingStyles:NSSet.set];
  NSArray *quote=request[@"quote"];
  if(quote)[blocks setBlockType:ENRMInputBlockTypeQuote level:1 forParagraphRange:NSMakeRange([quote[0] unsignedIntegerValue],[quote[1] unsignedIntegerValue]) inText:text];
  NSString *markdown=generic ? [ENRMMarkdownSerializer serializePlainText:text ranges:styles.allRanges] :
    [ENRMMarkdownSerializer serializeComposerText:text ranges:styles.allRanges blockRanges:blocks.allRanges];
  NSMutableArray *types=[NSMutableArray new];for(ENRMFormattingRange *range in styles.allRanges)[types addObject:@(range.type)];
  NSData *output=[NSJSONSerialization dataWithJSONObject:@{@"plain":text,@"markdown":markdown,@"types":types} options:0 error:&error];
  if(error)return 4;[[NSFileHandle fileHandleWithStandardOutput] writeData:output];return 0;
}}
