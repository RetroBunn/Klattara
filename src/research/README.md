# Klattara research papers

This folder holds openly available papers and one small data set for building a Klatt-style formant text-to-speech synthesizer for American English. Every file was downloaded from a free source: an author's or institution's own page, an open proceedings archive, a government archive, or an Internet Archive copy of one of those when the original server was down or blocked scripted downloads. Each PDF was checked to start with "%PDF" and to end with a complete "%%EOF" marker. Nothing was opened or run except to read its text.

Files are grouped by topic, in the project's priority order. Each entry gives the file name, the citation, where it came from, how it is licensed or hosted, and which part of the synthesizer it informs.

## Before you start: two notes

### Screen reader access

Some files are scanned page images with no text layer, so a screen reader will read nothing or almost nothing. They need OCR before you can read them. These are:

- Anderson_1984_SynthesisByRuleEnglishIntonation.pdf (only the cover sheet has text)
- Hertz_1992_NucleusBasedTimingModel.pdf
- Hertz_1995_DeltaSystemWithSyllt.pdf
- Pierrehumbert_1990_MeaningOfIntonationalContours.pdf (only the first page has text)
- Silverman_1992_ToBIStandardEnglishProsody.pdf
- Stevens_1994_ControlKlattSynthesizerArticulatoryParameters.pdf
- Sproat_1999_NormalizationNonStandardWordsWS99Report.pdf (it has a text layer, but the font encoding makes it come out as garbage)

These files have a text layer from old OCR. It is readable, but expect some errors, such as words with spaces between the letters:

- Klatt_1979_SynthesisByRuleSegmentalDurations.pdf
- Carlson_1979_PerceptionTemporalPatterns.pdf
- Hunnicutt_1976_PhonologicalRulesTextToSpeech.pdf
- Elovitz_1976_LetterToSoundRules.pdf
- Pierrehumbert_1980_PhonologyPhoneticsEnglishIntonation.pdf
- Pisoni_1983_ResearchOnSpeechPerceptionPR9_SpeechRatePitchContour.pdf
- Bachenko_1990_DiscourseNeutralProsodicPhrasing.pdf

### Copyright

Most of these papers are still under copyright. They are fine to keep for private study, but they should not be committed to the public open-source repository. Consider adding src/research/*.pdf to .gitignore and committing only this README. The exceptions are:

- The NRL report by Elovitz and colleagues and the Klatt US patent are US government publications.
- The MIT OpenCourseWare ToBI text is licensed CC BY-NC-SA.
- MIT says its theses may be viewed for any purpose but must not be redistributed without permission. That applies to the Pierrehumbert thesis.

## Topic 1: The Klatt synthesizer and formant synthesis

### Stevens_1991_KlattsLegacySpeechSynthesis.pdf

- Citation: Stevens, K. N. (1991). The contribution of speech synthesis to phonetics: Dennis Klatt's legacy. Proceedings of the 12th International Congress of Phonetic Sciences (ICPhS XII), Aix-en-Provence, Vol. 1, pp. 28-37.
- Source: https://www.coli.uni-saarland.de/Phonetics/icphs/ICPhS1991/12_ICPhS_1991_Vol_1/p12.1_028.pdf
- Access: ICPhS proceedings archive hosted by Saarland University phonetics.
- Informs: The overall KLSYN88 design: glottal source, frication source, the cascade and parallel branches, and how to control them. This is the best open overview of the synthesizer.

### Stevens_1994_ControlKlattSynthesizerArticulatoryParameters.pdf

- Citation: Stevens, K. N., Bickley, C. A., and Williams, D. R. (1994). Control of a Klatt synthesizer by articulatory parameters. Proceedings of ICSLP 1994, Yokohama, pp. 183-186. DOI 10.21437/ICSLP.1994-49
- Source: https://www.isca-archive.org/icslp_1994/stevens94_icslp.pdf
- Access: ISCA Archive, open. Scanned, so it needs OCR.
- Informs: Driving the many Klatt parameters from a small set of higher-level, articulation-like controls, which keeps the rules simpler and more consistent.

### Klatt_1988_USPatent4754485_TTSDigitalProcessor.pdf

- Citation: Klatt, D. H. (1988). Digital processor for use in a text to speech system. US Patent 4,754,485. Filed 12 December 1983, granted 28 June 1988. Assignee: Digital Equipment Corporation.
- Source: https://patentimages.storage.googleapis.com/32/56/52/bc920735cac7d6/US4754485.pdf (record page: https://patents.google.com/patent/US4754485A/en)
- Access: US patent, a public government document. This is Google Patents' copy of the USPTO file.
- Informs: How DECtalk ran the synthesizer: parameter frames every 6.4 ms, and a glottal waveform generated at 40 kHz, then low-pass filtered and downsampled to 10 kHz to avoid aliasing at high pitch.

### Gold_1968_DigitalAnalogFormantSynthesizers.pdf

- Citation: Gold, B., and Rabiner, L. R. (1968). Analysis of digital and analog formant synthesizers. IEEE Transactions on Audio and Electroacoustics, AU-16(1), 81-94. DOI 10.1109/TAU.1968.1161954
- Source: https://www.ece.ucsb.edu/Faculty/Rabiner/ece259/Reprints/006_synthesizers.pdf
- Access: Lawrence Rabiner's own publications page at UC Santa Barbara.
- Informs: The math of the digital two-pole resonator that Klatt's cascade branch is built from, its frequency response, and how to correct for formants left out of the cascade.

## Topic 2: Glottal source

### Fant_1985_FourParameterGlottalFlowModel.pdf

- Citation: Fant, G., Liljencrants, J., and Lin, Q. (1985). A four-parameter model of glottal flow. STL-QPSR 26(4), 1-13. KTH Speech Transmission Laboratory, Stockholm.
- Source: https://www.speech.kth.se/qpsr/1985/1985_26_4_001-013.pdf
- Access: KTH Speech, Music and Hearing open QPSR archive.
- Informs: The LF model of the glottal flow derivative, a candidate voicing source to replace or add to the Klatt source.

### Fant_1995_LFModelRevisited.pdf

- Citation: Fant, G. (1995). The LF-model revisited. Transformations and frequency domain analysis. STL-QPSR 36(2-3), 119-156.
- Source: https://www.speech.kth.se/qpsr/1995/1995_36_2-3_119-156.pdf
- Access: KTH open QPSR archive.
- Informs: Practical LF formulas, the single voice-quality parameter Rd, and what each parameter does to the spectrum. Useful for a simple, predictable voice-quality control.

### Gobl_1988_VoiceSourceDynamicsConnectedSpeech.pdf

- Citation: Gobl, C. (1988). Voice source dynamics in connected speech. STL-QPSR 29(1), 123-159.
- Source: https://www.speech.kth.se/qpsr/1988/1988_29_1_123-159.pdf
- Access: KTH open QPSR archive.
- Informs: How source parameters change with stress, position in the phrase and nearby consonants, so the voice does not sound static.

### Carlson_1990_VoiceModellingInSpeechSynthesis.pdf

- Citation: Carlson, R., Granström, B., and Karlsson, I. (1990). Experiments with voice modelling in speech synthesis. STL-QPSR 31(2-3), 53-61.
- Source: https://www.speech.kth.se/qpsr/1990/1990_31_2-3_053-061.pdf
- Access: KTH open QPSR archive.
- Informs: Using an LF-type source inside a rule-based formant text-to-speech system, including female voices and source changes over time.

### Garellek_2016_TGlottalizationPhrasalCreak.pdf

- Citation: Garellek, M., and Seyfarth, S. (2016). Acoustic differences between English /t/ glottalization and phrasal creak. Proceedings of Interspeech 2016, San Francisco, pp. 1054-1058. DOI 10.21437/Interspeech.2016-1472
- Source: https://www.isca-archive.org/interspeech_2016/garellek16_interspeech.pdf
- Access: ISCA Archive, open.
- Informs: In the Buckeye corpus of American conversation, about 60% of syllable-final /t/s after a vowel are glottalized: the vowel ends in irregular, noisier voicing with falling F0, often with no /t/ release. Klattara's glottalized /t/ follows this: the voice creaks over the end of the vowel, lower and uneven, and the release is kept as a cue to /t/ rather than /d/.

## Topic 3: Segmental duration and speaking rate

### Klatt_1979_SynthesisByRuleSegmentalDurations.pdf

- Citation: Klatt, D. H. (1979). Synthesis by rule of segmental durations in English sentences. Proceedings of the 9th International Congress of Phonetic Sciences, Copenhagen, Vol. 2 (Symposium 5), pp. 290-297.
- Source: https://www.coli.uni-saarland.de/Phonetics/icphs/ICPhS1979/p9.2_290.pdf
- Access: ICPhS proceedings archive hosted by Saarland University.
- Informs: The Klatt duration rules themselves: an inherent and a minimum duration for each phone, then percentage rules for pre-pausal lengthening, non-final shortening, polysyllabic shortening, stress, clusters and so on. This is the core of the duration module. It is the conference version of the book chapter listed under "Wanted".

### Carlson_1979_PerceptionTemporalPatterns.pdf

- Citation: Carlson, R., Granström, B., and Klatt, D. H. (1979). Some notes on the perception of temporal patterns in speech. Proceedings of ICPhS 9, Copenhagen, Vol. 2, pp. 260-267.
- Source: https://www.coli.uni-saarland.de/Phonetics/icphs/ICPhS1979/p9.2_260.pdf
- Access: ICPhS archive at Saarland University.
- Informs: Listening tests on the Klatt duration rules, and how much timing error listeners notice.

### Clements_1995_EnglishVowelDurationModel.pdf

- Citation: Clements, G. N., Hertz, S. R., and Lauret, B. (1995). A representational basis for modelling English vowel duration. Proceedings of ICPhS 13, Stockholm, Vol. 1, pp. 46-49.
- Source: https://www.coli.uni-saarland.de/Phonetics/icphs/ICPhS1995/13_ICPhS_1995_Vol_1/p13.1_046.pdf
- Access: ICPhS archive at Saarland University.
- Informs: A syllable-structure-based way to predict English vowel duration, from Hertz's group. An alternative or check on the Klatt rules.

### Hertz_1992_NucleusBasedTimingModel.pdf

- Citation: Hertz, S. R., and Huffman, M. K. (1992). A nucleus-based timing model applied to multi-dialect speech synthesis by rule. Proceedings of ICSLP 1992, Banff, pp. 1171-1174. DOI 10.21437/ICSLP.1992-143
- Source: https://www.isca-archive.org/icslp_1992/hertz92_icslp.pdf
- Access: ISCA Archive, open. Scanned, so it needs OCR.
- Informs: Hertz's timing model, which builds durations around the syllable nucleus. It also shows her rule framework in use (topic 6).

### Brinckmann_2003_DurationModelsTimingSyntheticSpeech.pdf

- Citation: Brinckmann, C., and Trouvain, J. (2003). The role of duration models and symbolic representation for timing in synthetic speech. International Journal of Speech Technology 6(1), 21-31. DOI 10.1023/A:1021043804581
- Source: https://www.coli.uni-saarland.de/~trouvain/brinckmann_trouvain_2003.pdf
- Access: Jürgen Trouvain's own publications page (he is the co-author).
- Informs: Listening tests comparing Klatt rules with a trained tree model. Getting the phone string and reductions right mattered more than the choice of duration model.

### Trouvain_2004_TempoVariationSpeechProductionSynthesis.pdf

- Citation: Trouvain, J. (2004). Tempo variation in speech production: Implications for speech synthesis. Doctoral dissertation, Saarland University. Published as Phonus 8, Institute of Phonetics, Saarbrücken.
- Source: https://www.coli.uni-saarland.de/groups/BM/phonetics/contents/phonus-pdf/phonus8/Trouvain_Phonus8.pdf
- Access: Saarland University phonetics, the author's institution.
- Informs: How people change pauses, phrasing and articulation rate when they speed up or slow down, and how to model that in a synthesizer. This is the main source for rate control (it also belongs under topic 9).

### Trouvain_2002_TempoControlProsodicPhrasing.pdf

- Citation: Trouvain, J. (2002). Tempo control in speech synthesis by prosodic phrasing. Proceedings of KONVENS 2002, Saarbrücken (DFKI Document D-02-01), pp. 227-230.
- Source: https://www.coli.uni-saarland.de/~trouvain/trouvain_2002_konvens.pdf
- Access: author's own page.
- Informs: Changing speaking rate by changing the number and length of pauses and phrases, not only by squeezing segments. This bears on keeping commas and periods audible at high rates.

### Luce_1985_VowelClosureDurationVoicingFinalStops.pdf

- Citation: Luce, P. A., and Charles-Luce, J. (1985). Contextual effects on vowel duration, closure duration, and the consonant/vowel ratio in speech production. Journal of the Acoustical Society of America 78(6), 1949-1957. DOI 10.1121/1.392651
- Source: https://www.acsu.buffalo.edu/~luce/pdfs/1985-Luce&Charles-Luce.pdf
- Access: the author's page. Text layer from old OCR; the tables are garbled but readable.
- Informs: How much longer a vowel is before a voiced final stop than before a voiceless one: about 1.15 to 1.27 times in mid-sentence and 1.35 to 1.8 at the end of a phrase. Vowel length was the most reliable voicing cue. Final /p/ and /k/ close 1.5 to 1.8 times longer than /b/ and /g/, while /t/ and /d/ barely differ. A check on Klatt's rule 9 outside phrase ends.

## Topic 4: Intonation

### Pierrehumbert_1980_PhonologyPhoneticsEnglishIntonation.pdf

- Citation: Pierrehumbert, J. B. (1980). The phonology and phonetics of English intonation. PhD thesis, Department of Linguistics and Philosophy, MIT.
- Source: MIT DSpace record https://dspace.mit.edu/handle/1721.1/16065. The file came from the Internet Archive's copy of the DSpace file, http://dspace.mit.edu/bitstream/handle/1721.1/16065/07492108-MIT.pdf, captured 25 December 2023, because DSpace was rate-limiting downloads.
- Access: MIT DSpace thesis. Free to view; MIT asks that it not be redistributed.
- Informs: The tone inventory (pitch accents, phrase accents, boundary tones) and the rules that turn tones into F0 (downstep, declination, interpolation). These are the basis for statement, question and continuation contours.

### Pierrehumbert_1981_SynthesizingIntonation.pdf

- Citation: Pierrehumbert, J. (1981). Synthesizing intonation. Journal of the Acoustical Society of America 70(4), 985-995. DOI 10.1121/1.387033
- Source: https://www.phon.ox.ac.uk/jpierrehumbert/publications/synthesizing_intonation.pdf
- Access: author's own publications page at Oxford.
- Informs: A working F0 synthesis algorithm: target levels for each tone, the transitions between them, and the declining baseline.

### Anderson_1984_SynthesisByRuleEnglishIntonation.pdf

- Citation: Anderson, M., Pierrehumbert, J., and Liberman, M. (1984). Synthesis by rule of English intonation patterns. Proceedings of ICASSP 84, San Diego, Vol. 9, pp. 77-80 (printed as 2.8.1-2.8.4). DOI 10.1109/ICASSP.1984.1172427
- Source: https://www.phon.ox.ac.uk/jpierrehumbert/publications/AndersonPierrehumbertLiberman1984.pdf
- Access: Pierrehumbert's own publications page. Scanned, so it needs OCR.
- Informs: The rule system that turns a tone sequence into an F0 contour for text-to-speech. It applies directly to choosing a tune from punctuation.

### Pierrehumbert_1990_MeaningOfIntonationalContours.pdf

- Citation: Pierrehumbert, J., and Hirschberg, J. (1990). The meaning of intonational contours in the interpretation of discourse. In P. Cohen, J. Morgan, and M. Pollack (Eds.), Intentions in Communication, pp. 271-311. MIT Press.
- Source: https://www.phon.ox.ac.uk/jpierrehumbert/publications/PierrehumbertHirschberg1990.pdf
- Access: Pierrehumbert's own publications page. Scanned after the first page, so it needs OCR.
- Informs: What each tune signals, for example falling statements, rising yes-no questions and continuation rises. This guides the mapping from sentence type and punctuation to tunes.

### Silverman_1992_ToBIStandardEnglishProsody.pdf

- Citation: Silverman, K., Beckman, M., Pitrelli, J., Ostendorf, M., Wightman, C., Price, P., Pierrehumbert, J., and Hirschberg, J. (1992). ToBI: A standard for labeling English prosody. Proceedings of ICSLP 1992, Banff, pp. 867-870. DOI 10.21437/ICSLP.1992-260
- Source: https://www.isca-archive.org/icslp_1992/silverman92_icslp.pdf
- Access: ISCA Archive, open. Scanned, so it needs OCR.
- Informs: The standard American English set of tones and break indices, a shared vocabulary for writing intonation rules.

### Veilleux_2006_ToBITranscriptionCourseText_MITOCW.pdf

- Citation: Veilleux, N., Shattuck-Hufnagel, S., and Brugos, A. (2006). Transcribing Prosodic Structure of Spoken Utterances with ToBI. Course text for MIT 6.911, January IAP 2006. MIT OpenCourseWare.
- Source: https://ocw.mit.edu/courses/6-911-transcribing-prosodic-structure-of-spoken-utterances-with-tobi-january-iap-2006/6_911_iap_06_coursetextbook.pdf
- Access: MIT OpenCourseWare, Creative Commons BY-NC-SA licence. Good text layer.
- Informs: A readable, example-by-example guide to each pitch accent and boundary tone, including question rises and continuation rises. It refers to audio examples that are not included here.

### Hedberg_nd_MeaningsConfigurationsQuestionsEnglish.pdf

- Citation: Hedberg, N., Sosa, J. M., and Fadden, L. Meanings and configurations of questions in English. Simon Fraser University. The paper carries no date or venue; it cites work from 2003.
- Source: https://www.cs.columbia.edu/~julia/papers/hedbergetal.pdf
- Access: hosted on Julia Hirschberg's paper page at Columbia University. Good text layer.
- Informs: How American English questions end, from 113 telephone conversations (CallHome): 82% of wh-questions (with a fronted question word) fall, while most yes-no questions and declarative questions rise. This is the basis for choosing the ending of each kind of question.

### Hedberg_2014_YesNoQuestionIntonationCorpusStudy.pdf

- Citation: Hedberg, N., Sosa, J. M., and Görgülü, E. The meaning of intonation in yes-no questions in American English: A corpus study. Corpus Linguistics and Linguistic Theory. DOI 10.1515/cllt-2014-0020
- Source: https://www.sfu.ca/~hedberg/Hedberg_Sosa_Gorgulu_2014.pdf
- Access: author's copy on Hedberg's Simon Fraser University page. Good text layer.
- Informs: In 410 natural yes-no questions, the low rise is by far the most common ending; falling yes-no questions are rare and are not genuine questions (rhetorical, or the answer is already known). This supports keeping the rise on yes-no questions.

### Black_1996_F0ContoursFromToBILinearRegression.pdf

- Citation: Black, A. W., and Hunt, A. J. (1996). Generating F0 contours from ToBI labels using linear regression. Proceedings of ICSLP 1996, Philadelphia, pp. 1385-1388. DOI 10.21437/ICSLP.1996-354
- Source: https://www.isca-archive.org/icslp_1996/black96_icslp.pdf
- Access: ISCA Archive, open.
- Informs: A simple, predictable way to turn accent and boundary labels into F0 targets at the start, middle and end of each syllable.

### Bachenko_1990_DiscourseNeutralProsodicPhrasing.pdf

- Citation: Bachenko, J., and Fitzpatrick, E. (1990). A computational grammar of discourse-neutral prosodic phrasing in English. Computational Linguistics 16(3), 155-170.
- Source: https://aclanthology.org/J90-3003.pdf
- Access: ACL Anthology, open. Text layer from old OCR, with some errors.
- Informs: Where to break long sentences into phrases. Breaks go before a new group (a preposition, a clause, "and" joining phrases), and the parts are balanced in length around the verb. A final prepositional phrase of one stressed word is rarely set off; a longer one usually is. A wrong break misleads more than a missing one helps. The basis for the phrase breaks in text.rs.

### OShaughnessy_1989_ParsingSmallDictionaryTextToSpeech.pdf

- Citation: O'Shaughnessy, D. D. (1989). Parsing with a small dictionary for applications such as text to speech. Computational Linguistics 15(2), 97-108.
- Source: https://aclanthology.org/J89-2002.pdf
- Access: ACL Anthology, open.
- Informs: Finding phrase groups from function words alone, without a full grammar parser, as Klattara must. Pauses are rare inside a noun group, verb group or prepositional phrase, and likely between them; speakers break long stretches more than short ones.

### Riley_1994_TextAnalysisToolsSpokenLanguageProcessing.pdf

- Citation: Riley, M., and Sproat, R. (1994). Text analysis tools in spoken language processing. Tutorial notes, ACL 1994, AT&T Bell Laboratories. arXiv cmp-lg/9406036.
- Source: https://arxiv.org/pdf/cmp-lg/9406036
- Access: arXiv, open. Slides, so the text is in short fragments.
- Informs: Bell Labs' hand-derived accent rules (Hirschberg 1993) in order: function words unaccented, proper names accented, words already in focus ("given") unaccented, complex nominals by their citation accent, everything else accented; and their accuracy, about 80% to 82% of accents right on read corpora. The basis for the clause-word and repeated-word accent rules in text.rs.

## Topic 5: Letter-to-sound rules and stress

### Elovitz_1976_LetterToSoundRules.pdf

- Citation: Elovitz, H. S., Johnson, R. W., McHugh, A., and Shore, J. E. (1976). Automatic translation of English text to phonetics by means of letter-to-sound rules. NRL Report 7948, Naval Research Laboratory, Washington, DC, 21 January 1976. DTIC accession AD-A021929.
- Source: https://apps.dtic.mil/sti/pdfs/ADA021929.pdf. DTIC was down for maintenance, so the file came from the Internet Archive's copy of that exact URL, captured 29 April 2024.
- Access: DTIC, approved for public release, distribution unlimited. US government work.
- Informs: The complete NRL letter-to-sound rule set and its rule format. A predictable fallback for words that are not in the dictionary.

### Hunnicutt_1976_PhonologicalRulesTextToSpeech.pdf

- Citation: Hunnicutt, S. (1976). Phonological rules for a text-to-speech system. American Journal of Computational Linguistics, Microfiche 57.
- Source: https://aclanthology.org/J76-4008.pdf
- Access: ACL Anthology, open. Scanned microfiche with rough OCR, 81 pages.
- Informs: The MIT text-to-speech letter-to-sound and lexical stress rules, including affix handling. These are the ancestors of MITalk and Klattalk.

### Hunnicutt_1980_GraphemeToPhonemeRulesReview.pdf

- Citation: Hunnicutt, S. (1980). Grapheme-to-phoneme rules: A review. STL-QPSR 21(2-3), 38-60.
- Source: https://www.speech.kth.se/qpsr/1980/1980_21_2-3_038-060.pdf
- Access: KTH open QPSR archive.
- Informs: Compares the rule systems of the time, including NRL and MIT, and the kinds of errors each makes.

### Church_1985_StressAssignmentLetterToSound.pdf

- Citation: Church, K. W. (1985). Stress assignment in letter to sound rules for speech synthesis. Proceedings of the 23rd Annual Meeting of the Association for Computational Linguistics, Chicago, pp. 246-253. DOI 10.3115/981210.981240
- Source: https://aclanthology.org/P85-1030.pdf
- Access: ACL Anthology, open.
- Informs: Predicting stress from syllable weight, part of speech, morphology and word origin. This matters for predictable pronunciation of unknown words.

### Black_1998_IssuesBuildingLetterToSoundRules.pdf

- Citation: Black, A. W., Lenzo, K., and Pagel, V. (1998). Issues in building general letter to sound rules. Proceedings of the 3rd ESCA/COCOSDA Workshop on Speech Synthesis (SSW3), Jenolan Caves, Australia, pp. 77-80.
- Source: https://www.isca-archive.org/ssw_1998/black98_ssw.pdf
- Access: ISCA Archive (ESCA workshop), open.
- Informs: Training letter-to-sound decision trees automatically from a pronouncing dictionary such as CMUdict, including letter-phone alignment.

### Bisani_2002_JointMultigramGraphemeToPhoneme.pdf

- Citation: Bisani, M., and Ney, H. (2002). Investigations on joint-multigram models for grapheme-to-phoneme conversion. Proceedings of ICSLP 2002, Denver, pp. 105-108. DOI 10.21437/ICSLP.2002-78
- Source: https://www.isca-archive.org/icslp_2002/bisani02_icslp.pdf
- Access: ISCA Archive, open.
- Informs: Joint-sequence (graphone) models for grapheme-to-phoneme conversion. This is the early version of the paywalled 2008 journal paper.

### Novak_2012_WFSTGraphemeToPhonemeOpenSourceTools.pdf

- Citation: Novak, J. R., Minematsu, N., and Hirose, K. (2012). WFST-based grapheme-to-phoneme conversion: Open source tools for alignment, model-building and decoding. Proceedings of the 10th International Workshop on Finite State Methods and Natural Language Processing (FSMNLP), Donostia-San Sebastián, pp. 45-49.
- Source: https://aclanthology.org/W12-6208.pdf
- Access: ACL Anthology, open.
- Informs: Phonetisaurus: alignment and joint n-gram models as finite-state transducers. An option for words that are neither in the dictionary nor well handled by the rules.

## Topic 6: Rule-synthesis frameworks

### Carlson_1975_TextToSpeechPhoneticProgrammingLanguage.pdf

- Citation: Carlson, R., and Granström, B. (1975). A text-to-speech system based on a phonetically oriented programming language. STL-QPSR 16(1), 17-26.
- Source: https://www.speech.kth.se/qpsr/1975/1975_16_1_017-026.pdf
- Access: KTH open QPSR archive.
- Informs: The KTH approach: synthesis rules written in a phonetic notation that drive a formant synthesizer. A model for how to write and organize our own rules.

### Rabiner_1969_ModelForSynthesizingSpeechByRule.pdf

- Citation: Rabiner, L. R. (1969). A model for synthesizing speech by rule. IEEE Transactions on Audio and Electroacoustics, AU-17(1), 7-13. DOI 10.1109/TAU.1969.1162020
- Source: https://www.ece.ucsb.edu/Faculty/Rabiner/ece259/Reprints/013_SBR.pdf
- Access: Lawrence Rabiner's own publications page.
- Informs: Synthesis by rule in the acoustic domain: formant targets for each phoneme and smooth transition functions between them. It is based on his 1967 MIT thesis.

### Hertz_1995_DeltaSystemWithSyllt.pdf

- Citation: Hertz, S. R., and Zsiga, E. C. (1995). The Delta system with Syllt: Increased capabilities for teaching and research in phonetics. Proceedings of ICPhS 13, Stockholm, Vol. 2, pp. 322-325.
- Source: https://www.coli.uni-saarland.de/Phonetics/icphs/ICPhS1995/13_ICPhS_1995_Vol_2/p13.2_322.pdf
- Access: ICPhS archive at Saarland University. Scanned, so it needs OCR.
- Informs: Hertz's Delta system, which keeps phonological and phonetic information in parallel synchronized streams. This is the framework behind "Streams, phones and transitions". See also Hertz_1992 under topic 3.

## Topic 8: Text normalization

### Sproat_1999_NormalizationNonStandardWordsWS99Report.pdf

- Citation: Sproat, R., Black, A. W., Chen, S., Kumar, S., Ostendorf, M., and Richards, C. (1999). Normalization of non-standard words: WS'99 final report. Center for Language and Speech Processing, Johns Hopkins University.
- Source: http://www.clsp.jhu.edu/ws99/projects/normal/report/report.pdf. The original link no longer works, so the file came from the Internet Archive's capture of it, dated 30 September 2000.
- Access: open JHU CLSP workshop report. The text layer comes out as garbage, so it needs OCR (78 pages).
- Informs: A taxonomy of non-standard words (numbers, dates, money, abbreviations, acronyms, URLs) and how to expand each into words.

### SproatJaitly_2016_RNNTextNormalizationChallenge.pdf

- Citation: Sproat, R., and Jaitly, N. (2016). RNN approaches to text normalization: A challenge. arXiv:1611.00068. A later version appeared as "An RNN model of text normalization", Interspeech 2017.
- Source: https://arxiv.org/pdf/1611.00068
- Access: arXiv, open. Clean text layer.
- Informs: Describes a large set of Wikipedia text aligned with how Google's Kestrel system reads it aloud, sorted by class: dates, cardinal and ordinal numbers, measures, letters, Roman numerals and more. That data is a ready test set for our text front end. The authors link it from https://github.com/rwsproat/text-normalization-data, now hosted on Kaggle, which needs an account; the old Google Storage link returns "403 Forbidden". It also finds that neural models make a few badly wrong readings that rules prevent. Kaggle lists the data's license as unknown, so it is used only privately, to measure: src/tools/normalization_eval.py scores against it, and nothing taken from it ships with Klattara.

### Zhang_2019_NeuralTextNormalizationSpeech.pdf

- Citation: Zhang, H., Sproat, R., Ng, A. H., Stahlberg, F., Peng, X., Gorman, K., and Roark, B. (2019). Neural models of text normalization for speech applications. Computational Linguistics 45(2), 293-337. DOI 10.1162/coli_a_00349
- Source: https://aclanthology.org/J19-2004.pdf
- Access: ACL Anthology, licensed CC BY-NC-ND 4.0. Clean text layer.
- Informs: The fullest account of Google's text normalization work: which classes are hard, which errors are unacceptable, such as misreading a number, and how hand-written grammars keep a learned system from making them. Also defines the standard test split of the data above.

### Yarowsky_1997_HomographDisambiguationTTS.pdf

- Citation: Yarowsky, D. (1997). Homograph disambiguation in text-to-speech synthesis. In van Santen, J., Sproat, R., Olive, J., and Hirschberg, J. (eds.), Progress in Speech Synthesis, pp. 157-172. Springer. DOI 10.1007/978-1-4612-1894-4_12
- Source: https://www.cs.columbia.edu/~julia/papers/yarowsky97.pdf, a copy on Julia Hirschberg's Columbia page (she co-edited the book), not the author's own page.
- Access: Columbia course page. Clean text layer.
- Informs: The classic method for homographs in TTS: decision lists that rank clues from nearby words and wider context by how reliably they predict each reading. It covers noun/verb stress pairs, "read" by tense, and words like "lead" and "bass". This is the same kind of rule our homograph module uses, but learned from a large corpus.

### Gorman_2018_HomographDisambiguationSupervisedML.pdf

- Citation: Gorman, K., Mazovetskiy, G., and Nikolaev, V. (2018). Improving homograph disambiguation with supervised machine learning. Proceedings of LREC 2018, Miyazaki, pp. 1349-1352.
- Source: https://aclanthology.org/L18-1215.pdf
- Access: ACL Anthology, open. Clean text layer.
- Informs: Google's homograph system, where rules plus a learned classifier beat either alone. It comes with the Wikipedia homograph data (https://github.com/google-research-datasets/WikipediaHomographData, Apache 2.0): 162 homographs with about 100 labeled sentences each, split into training and test sets. src/tools/homograph_eval.py measures Klattara against it; the data goes in eval/wikipedia_homographs, which git ignores.

## Topic 9: Fast speech and blind listeners

### Moos_2007_ComprehensionUltraFastSpeechBlind.pdf

- Citation: Moos, A., and Trouvain, J. (2007). Comprehension of ultra-fast speech: blind vs. "normally hearing" persons. Proceedings of ICPhS XVI, Saarbrücken, pp. 677-680.
- Source: http://www.icphs2007.de/conference/Papers/1186/1186.pdf (linked from Trouvain's publications page)
- Access: official ICPhS 2007 conference site.
- Informs: How fast blind screen-reader users actually listen, in syllables per second, and where understanding breaks down. These numbers set the target range for our speed control.

### Trouvain_2007_ComprehensionExtremelyFastSyntheticSpeech.pdf

- Citation: Trouvain, J. (2007). On the comprehension of extremely fast synthetic speech. Saarland Working Papers in Linguistics 1, 5-13.
- Source: https://publikationen.sulb.uni-saarland.de/bitstream/20.500.11880/23553/1/Trouvain.pdf. The repository blocks scripted downloads with a bot check, so the file came from the Internet Archive's capture of that URL, dated 18 April 2024.
- Access: Saarland University open repository.
- Informs: Blind listeners understood formant synthesis at every rate tested, up to 17.5 syllables per second. Above 7.5 syllables per second, formant synthesis was easier to follow than diphone synthesis. This is direct support for the formant approach.

### ValentiniBotinhao_2014_IntelligibilityFastSynthesizedSpeech.pdf

- Citation: Valentini-Botinhao, C., Toman, M., Pucher, M., Schabus, D., and Yamagishi, J. (2014). Intelligibility analysis of fast synthesized speech. Proceedings of Interspeech 2014, Singapore, pp. 2922-2926. DOI 10.21437/Interspeech.2014-197
- Source: https://www.isca-archive.org/interspeech_2014/valentinibotinhao14_interspeech.pdf
- Access: ISCA Archive, open.
- Informs: Compares ways of speeding up synthetic speech with blind and sighted listeners. Plain linear compression was the most intelligible at the highest rates, which is a useful baseline for fast-rate duration rules.

### Carlson_1976_TTSReadingMachineForBlindEvaluation.pdf

- Citation: Carlson, R., Granström, B., and Larsson, K. (1976). Evaluation of a text-to-speech system as a reading machine for the blind. STL-QPSR 17(2-3), 9-13.
- Source: https://www.speech.kth.se/qpsr/1976/1976_17_2-3_009-013.pdf
- Access: KTH open QPSR archive.
- Informs: An early evaluation of a rule-based formant text-to-speech system with blind readers, and what they needed from it.

### Neovius_1993_TTSComprehensionListeningSpeed.pdf

- Citation: Neovius, L., and Raghavendra, P. (1993). Evaluation of comprehension of KTH text-to-speech with "listening speed" paradigm. STL-QPSR 34(2-3), 21-30.
- Source: https://www.speech.kth.se/qpsr/1993/1993_34_2-3_021-030.pdf
- Access: KTH open QPSR archive.
- Informs: A test method that measures comprehension against listening speed for a formant synthesizer. We can reuse it to evaluate our own speed settings.

### Pisoni_1983_ResearchOnSpeechPerceptionPR9_SpeechRatePitchContour.pdf

- Citation: Pisoni, D. B., and others (1983). Research on Speech Perception, Progress Report No. 9, January-December 1983. Speech Research Laboratory, Indiana University, Bloomington. ERIC ED318055. The relevant paper is Slowiaczek, L. M., and Nusbaum, H. C., "Effects of speech rate and pitch contour on the perception of synthetic speech", which starts on PDF page 112.
- Source: https://files.eric.ed.gov/fulltext/ED318055.pdf. The same report is on the lab's page, https://pisoni.lab.indiana.edu/progress-reports-tab-3/progress-reports.html, but that copy has no text layer.
- Access: ERIC, US Department of Education. 357 pages.
- Informs: Measured intelligibility loss for rule-synthesized speech at 250 words per minute compared with 150, and the effect of a flat versus normal pitch contour. The rest of the report covers other synthetic speech perception studies.

## Topic 10: American English vowel formant data

### Hillenbrand_1995_AcousticCharacteristicsAmericanEnglishVowels.pdf

- Citation: Hillenbrand, J., Getty, L. A., Clark, M. J., and Wheeler, K. (1995). Acoustic characteristics of American English vowels. Journal of the Acoustical Society of America 97(5), 3099-3111. DOI 10.1121/1.411872
- Source: http://homepages.wmich.edu/~hillenbr/Papers/HillenbrandGettyClarkWheeler.pdf. The author's Western Michigan University site is now offline, so the file came from the Internet Archive's capture dated 15 June 2010.
- Access: author's own page (archived copy).
- Informs: F1 to F4, F0, duration and formant movement for 12 American English vowels from men, women and children. The main source for vowel targets.

### Hillenbrand_1995_vowdata.dat

- Citation: Data file accompanying Hillenbrand et al. (1995): measurements for 1,668 vowel tokens, in plain text.
- Source: http://homepages.wmich.edu/~hillenbr/voweldata/vowdata.dat, from the Internet Archive capture dated 10 May 2003. A copy is also kept, with the author's permission, at https://github.com/santiagobarreda/hillenbrand_et_al_1995
- Access: author's own data page (archived copy).
- Informs: Per-token values. The column layout is explained at the top of the file: duration, F0, F1 to F4 at steady state, and F1 to F3 at 20, 50 and 80 percent of the vowel.

### Hillenbrand_1995_vowdata_readme.txt

- Citation: Hillenbrand's readme for the vowel data folder.
- Source: http://homepages.wmich.edu/~hillenbr/voweldata/readme.txt, from the Internet Archive capture dated 10 May 2003.
- Access: author's own data page (archived copy).
- Informs: Explains the file naming (speaker group, talker number, vowel code) and lists the other data files, including the audio, which was not downloaded.

### Hillenbrand_1993_SteadyStateVowelsPetersonBarneySynthesized.pdf

- Citation: Hillenbrand, J., and Gayvert, R. T. (1993). Identification of steady-state vowels synthesized from the Peterson and Barney measurements. Journal of the Acoustical Society of America 94(2), 668-674. DOI 10.1121/1.406884
- Source: http://homepages.wmich.edu/~hillenbr/Papers/HillenbrandGayvertIdentificationOfSteadyStateVowelsSynthesizedFromThePetersonAndBarneyMeasurements.pdf, from the Internet Archive capture dated 15 June 2010.
- Access: author's own page (archived copy).
- Informs: Vowels resynthesized from the Peterson and Barney values with flat formants were misidentified 27 percent of the time. So the synthesizer needs vowel duration and formant movement, not just static targets.

### Peterson and Barney 1952 data

- Not downloaded as a separate file. The full table is built into Praat ("Create formant table (Peterson & Barney 1952)") and into the R package phonTools (data set pb52), so it can be exported from either when needed.

## Wanted but not openly available

These are key papers that appear to be available only behind a paywall or in print. No unofficial copies were used.

### Topic 1: the synthesizer

- Klatt, D. H. (1980). Software for a cascade/parallel formant synthesizer. Journal of the Acoustical Society of America 67(3), 971-995. DOI 10.1121/1.383940. Why: the defining description of the synthesizer, with resonator equations, the original voicing source, parameter list and tables of parameter values for English sounds. The most important missing source.
- Klatt, D. H., and Klatt, L. C. (1990). Analysis, synthesis, and perception of voice quality variations among female and male talkers. Journal of the Acoustical Society of America 87(2), 820-857. DOI 10.1121/1.398894. Why: documents KLSYN88, including the KLGLOTT88 voicing source (open quotient, spectral tilt, aspiration, flutter) and settings for female voices.
- Stevens, K. N., and Bickley, C. A. (1991). Constraints among parameters simplify control of Klatt formant synthesizer. Journal of Phonetics 19(1), 161-174. DOI 10.1016/S0095-4470(19)30310-9. Why: the journal version of the higher-level control scheme (HLsyn) in Stevens_1994.

### Topic 3: duration

- Klatt, D. H. (1976). Linguistic uses of segmental duration in English: Acoustic and perceptual evidence. Journal of the Acoustical Society of America 59(5), 1208-1221. DOI 10.1121/1.380986. Why: the data and perceptual thresholds behind the duration rules.
- Klatt, D. H. (1979). Synthesis by rule of segmental durations in English sentences. In B. Lindblom and S. Öhman (Eds.), Frontiers of Speech Communication Research, pp. 287-299. Academic Press. No DOI. Why: the book version of the duration rules. The ICPhS version, Klatt_1979, is in this folder and contains the rules.
- Klatt, D. H. (1976). Structure of a phonological rule component for a synthesis-by-rule program. IEEE Transactions on Acoustics, Speech, and Signal Processing 24(5), 391-398. DOI 10.1109/TASSP.1976.1162847. Why: how Klatt organized his phonological rules.

### Topic 4: intonation

- Liberman, M., and Pierrehumbert, J. (1984). Intonational invariance under changes in pitch range and length. In M. Aronoff and R. Oehrle (Eds.), Language Sound Structure, pp. 157-233. MIT Press. No DOI. Why: the pitch range and final-lowering model used by Anderson et al. (1984).
- Hirschberg, J. (1993). Pitch accent in context: predicting intonational prominence from text. Artificial Intelligence 63(1-2), 305-340. DOI 10.1016/0004-3702(93)90020-C. Why: the full account of the accent rules summarized in Riley_1994, with how "given" words are tracked and how well each rule does.
- Sproat, R. (1994). English noun-phrase accent prediction for text-to-speech. Computer Speech and Language 8(2), 79-94. Why: which word of a compound noun is accented ("CITY hall" against "city HALL"), the rule Klattara leaves out for lack of part-of-speech tagging.

### Topic 5: letter-to-sound

- Bisani, M., and Ney, H. (2008). Joint-sequence models for grapheme-to-phoneme conversion. Speech Communication 50(5), 434-451. DOI 10.1016/j.specom.2008.01.002. Why: the full description of the Sequitur model. The 2002 conference version is in this folder.
- Novak, J. R., Minematsu, N., and Hirose, K. (2016). Phonetisaurus: Exploring grapheme-to-phoneme conversion with joint n-gram models in the WFST framework. Natural Language Engineering 22(6), 907-938. DOI 10.1017/S1351324915000315. Why: the full Phonetisaurus description. The 2012 workshop paper is in this folder.

### Topic 6: rule frameworks

- Holmes, J. N., Mattingly, I. G., and Shearme, J. N. (1964). Speech synthesis by rule. Language and Speech 7(3), 127-143. DOI 10.1177/002383096400700301. Why: the classic table-driven target-and-transition scheme for formant synthesis by rule.
- Hertz, S. R. (1991). Streams, phones and transitions: toward a new phonological and phonetic model of formant timing. Journal of Phonetics 19(1), 91-109. DOI 10.1016/S0095-4470(19)30319-5. Why: Hertz's model of formant timing with phones and explicit transitions.
- Hertz, S. R. (1982). From text to speech with SRS. Journal of the Acoustical Society of America 72(4), 1155-1170. DOI 10.1121/1.388325. Why: her earlier complete rule-based text-to-speech system.
- Allen, J., Hunnicutt, M. S., and Klatt, D. (1987). From Text to Speech: The MITalk System. Cambridge University Press. A book, no DOI. Why: complete description of the MIT system (morph lexicon, letter-to-sound, stress, duration, F0, synthesizer).

### Topic 7: review

- Klatt, D. H. (1987). Review of text-to-speech conversion for English. Journal of the Acoustical Society of America 82(3), 737-793. DOI 10.1121/1.395275. Why: the broad review of the field, with many Klattalk rules. Copies exist on course websites, but none is a legitimate open copy, so none was used.
- Klatt, D. H. (1982). The Klattalk text-to-speech conversion system. Proceedings of ICASSP 82, pp. 1589-1592. DOI 10.1109/ICASSP.1982.1171431. Why: a short system description of Klattalk.

### Topic 8: text normalization

- Sproat, R., Black, A. W., Chen, S., Kumar, S., Ostendorf, M., and Richards, C. (2001). Normalization of non-standard words. Computer Speech and Language 15(3), 287-333. DOI 10.1006/csla.2001.0169. Why: the polished journal version of the WS'99 report in this folder. It is easier to read than the report, which needs OCR.

### Topic 9: fast speech and listeners

- Slowiaczek, L. M., and Nusbaum, H. C. (1985). Effects of speech rate and pitch contour on the perception of synthetic speech. Human Factors 27(6), 701-712. DOI 10.1177/001872088502700609. Why: journal version of the report chapter in the Pisoni_1983 file.
- Pisoni, D. B., Nusbaum, H. C., and Greene, B. G. (1985). Perception of synthetic speech generated by rule. Proceedings of the IEEE 73(11), 1665-1676. DOI 10.1109/PROC.1985.13346. Why: an overview of intelligibility and comprehension of rule-based synthesis, including DECtalk.
- Stent, A., Syrdal, A., and Mishra, T. (2011). On the intelligibility of fast synthesized speech for individuals with early-onset blindness. Proceedings of ASSETS 2011, pp. 211-218. DOI 10.1145/2049536.2049574. Why: compares formant and unit-selection synthesis at fast rates with early-blind listeners.
- Janse, E. (2004). Word perception in fast speech: artificially time-compressed vs. naturally produced fast speech. Speech Communication 42(2), 155-173. DOI 10.1016/j.specom.2003.07.001. Why: linear compression versus natural fast speech. An open copy may exist in the Utrecht University repository (https://dspace.library.uu.nl/handle/1874/14240), but its bot check blocked the download.

### Topic 10: vowel data

- Peterson, G. E., and Barney, H. L. (1952). Control methods used in a study of the vowels. Journal of the Acoustical Society of America 24(2), 175-184. DOI 10.1121/1.1906875. Why: the classic vowel formant study. Its data can be taken from Praat, as noted above.

## Checked but not downloaded

- MIT RLE Progress Reports, Speech Communication chapters, on DSpace. For example: No. 125 (1983), https://dspace.mit.edu/handle/1721.1/56870, and No. 129 (1987), https://dspace.mit.edu/handle/1721.1/56993. Each has only a paragraph on Klatt's synthesizer work. No. 125 mentions the new voicing-source controls for open quotient, corner rounding at closure, breathiness and jitter. Free, but skipped as too thin.
- Rabiner, L. R. (1967). Speech synthesis by rule: an acoustic domain approach. MIT PhD thesis, https://dspace.mit.edu/handle/1721.1/29182. Not fetched because DSpace was rate-limiting. The 1969 journal summary is included instead.
- MIT OpenCourseWare 6.542J lab handout on the Klatt synthesizer (Fall 2001). Free, but only two pages of lab instructions, so skipped.
- Haskins Laboratories: Mattingly, I. G. (1968), Synthesis by rule of General American English, supplement to the Status Report on Speech Research. No online copy was found.
- The Indiana Speech Research Laboratory page, https://pisoni.lab.indiana.edu/progress-reports-tab-3/progress-reports.html, has many more synthetic speech evaluations, including DECtalk studies in progress reports 10 and 11. Only report 9 was taken.
- CMUdict was deliberately not downloaded, as planned.
