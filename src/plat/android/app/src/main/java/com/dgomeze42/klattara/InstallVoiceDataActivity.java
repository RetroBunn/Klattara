package com.dgomeze42.klattara;

import android.app.Activity;
import android.os.Bundle;

/** Asked to install voice data: there is none to fetch, everything is in the app. */
public class InstallVoiceDataActivity extends Activity {
    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        setResult(RESULT_OK);
        finish();
    }
}
