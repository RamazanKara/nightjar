package com.limelight.grid;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import com.limelight.grid.PcGridAdapter.RowState;

import org.junit.Test;

/**
 * The PCs page has one amber control: the action of the first PC in the list while that PC is up. The list is sorted with the
 * PCs that are up first, so no PC further down can lead, and a PC that has to be woken or is still being looked at never does.
 */
public class PcLeadActionTest {
    @Test
    public void theFirstPcLeadsWhileItIsUp() {
        assertTrue("Resume", PcGridAdapter.leads(0, RowState.RUNNING));
        assertTrue("Open", PcGridAdapter.leads(0, RowState.READY));
        assertTrue("Pair", PcGridAdapter.leads(0, RowState.NOT_PAIRED));
    }

    @Test
    public void aPcThatIsAwayOrBeingCheckedNeverLeads() {
        assertFalse("Wake stays quiet", PcGridAdapter.leads(0, RowState.OFFLINE));
        assertFalse(PcGridAdapter.leads(0, RowState.WAKING));
        assertFalse(PcGridAdapter.leads(0, RowState.CHECKING));
    }

    @Test
    public void noSecondPcIsAmber() {
        for (RowState state : RowState.values()) {
            assertFalse(state.name(), PcGridAdapter.leads(1, state));
            assertFalse(state.name(), PcGridAdapter.leads(5, state));
        }
    }
}
