/**
 * Everyday English words, in their plain (singular) form. A note whose whole title is one of these
 * ("Ideas", "Meeting", "Plan") is not offered inline every time the word comes up in a sentence:
 * see `matchTitleBefore`. Plurals are folded before the lookup, so "ideas" finds "idea".
 *
 * Hand-picked rather than a frequency dump: the words people write all the time, weighted toward
 * the ones that also make likely note titles.
 */
const WORDS = `
a about above across act action active activity actually add address admin advice after afternoon
again against age agenda ago agree ahead air all allow almost alone along already also always
amount analysis and animal another answer any anyone anything app apple application approach area
argument arm around art article ask assignment attention audio author available average away baby
back background bad bag balance ball bank bar base basic be beach bear beautiful because become bed
before begin beginning behind belief believe below benefit best better between big bill bird birthday
bit black blog blue board boat body book bottom box boy brain break breakfast bring brother budget
bug build building business busy but buy by cake call camera camp can capital car card care career
case cash cat catch cause center central chair challenge chance change chapter character chart cheap
check checklist child choice choose church city class classic clean clear client climate clock close
cloud club coach code coffee cold collection college color come comment common community company
compare complete computer concept condition conference consider contact content context continue
contract control cook cool copy core corner cost could country couple course cover create creative
credit culture cup current customer cut cycle daily dark data date day deal dear death debt decide
decision deep default design desk detail develop development device diet difference different dinner
direction discussion doctor document dog door doubt down draft dream dress drink drive drop each
early earth easy eat economy edge edit education effect effort email end energy engine enough entry
environment error even evening event every everyone everything exam example exercise expense
experience experiment explain eye face fact fail fall family fast father fear feature feedback feel
few field fight figure file film final finance find fine finish fire first fish fit fix flight floor
flow focus follow food foot for force form format forward free friend from front fruit full fun
function fund future game garden gas general gift girl give glass goal good government great green
group grow growth guess guide gym habit hair half hand happy hard have head health hear heart help
here high hiring history hit hobby hold holiday home hope horse hospital hot hotel hour house how
human idea image impact important improve include income index info information input inside
insight instance interest internet interview into introduction inventory invoice issue item job join
journal journey just keep key kid kind kitchen know knowledge lab land language large last late
later law lead learn learning leave left lesson letter level library life light like limit line link
list listen little live local location log long look loss lot love low lunch machine main make man
manager map market marketing material matter maybe meal mean media medicine meet meeting member
memo memory message method middle mind minute mission model moment money month mood more morning
most mother move movie much music must name nature need network new news next nice night none note
nothing notebook now number object offer office often oil old one only open operation option or
order other out outline output outside over overview owner page pain paper parent park part party
pass past path pay people performance period person personal pet phone photo picture piece place
plan planning plant play player please podcast point policy poem political pool poor popular post
practice prep present price principle print priority private problem process product production
profile program progress project promise proposal public purpose put quality quarter question quick
quiet quote race radio rain random range rate reading real reason recipe record red reflection
relationship release remember report research resource response rest result retro return review
rich right risk road rock role room root rule run safe sale sample save say scene schedule school
science score search season seat second secret section security see self sell send sense series
service session set setup share shop short show side sign simple single sister site size skill sleep
small social software some someone something song soon sort sound source space speak special speech
spend sport spring staff stage standard start state statement status stay step still stock stop
store story strategy street strong structure student study stuff style subject success summary
summer sun support sure system table take talk target task tax tea teach team tech technology tell
template term test text thanks that the theme theory thing think this thought time tip title today
together tomorrow tonight tool top topic total touch town track trade training travel tree trip true
try turn type under understand unit until up update use user usual vacation value version video view
visit voice wait walk wall want war watch water way week weekend weekly weight well what when where
which while white who whole why wife will win window winter wish with woman word work workout world
write writing wrong yard year yellow yes yesterday yet you young zone
`;

/** The words, lower case. */
export const COMMON_WORDS: ReadonlySet<string> = new Set(WORDS.split(/\s+/).filter(Boolean));
