package com.dgomeze42.klattara;

import android.app.Activity;
import android.content.Intent;
import android.os.Bundle;
import android.speech.tts.TextToSpeech;

/** What Android's speech settings play when "Listen to an example" is chosen. */
public class GetSampleTextActivity extends Activity {
    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        Intent result = new Intent();
        result.putExtra("sampleText", "This is an example of speech synthesis with Klattara.");
        setResult(TextToSpeech.LANG_AVAILABLE, result);
        finish();
    }
}
