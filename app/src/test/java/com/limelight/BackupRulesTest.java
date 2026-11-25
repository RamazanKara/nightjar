package com.limelight;

import android.app.Application;
import android.content.res.XmlResourceParser;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;
import org.xmlpull.v1.XmlPullParser;

import static org.junit.Assert.assertTrue;

/** Tests Android backup and device-transfer exclusion rules. */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 31, application = Application.class)
public class BackupRulesTest {
    @Test
    public void backupRulesAndDataExtractionExcludeDatabasesAndSensitiveState() throws Exception {
        assertExcludesDatabase(R.xml.backup_rules);
        assertExcludesDatabase(R.xml.backup_rules_s);
    }

    private static void assertExcludesDatabase(int xmlRes) throws Exception {
        boolean foundDatabaseExclude = false;
        boolean foundSharedPrefExclude = false;
        try (XmlResourceParser parser = RuntimeEnvironment.getApplication().getResources().getXml(xmlRes)) {
            int event = parser.getEventType();
            while (event != XmlPullParser.END_DOCUMENT) {
                if (event == XmlPullParser.START_TAG && "exclude".equals(parser.getName())) {
                    String domain = parser.getAttributeValue(null, "domain");
                    if ("database".equals(domain)) foundDatabaseExclude = true;
                    if ("sharedpref".equals(domain)) foundSharedPrefExclude = true;
                }
                event = parser.next();
            }
        }
        assertTrue("database domain must be excluded from backup", foundDatabaseExclude);
        assertTrue("sharedpref domain must be excluded from backup", foundSharedPrefExclude);
    }
}
