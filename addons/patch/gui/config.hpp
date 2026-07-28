// A3SQL Patch Editor dialog — minimal RscDisplay using safeZone coordinates

// Custom Rsc base classes for HEMTT compat (avoids built-in Rsc* inheritance)
class RscText_custom {
    type = 0;
    style = 2;
    font = "RobotoCondensed";
    sizeEx = 0.04;
    colorText[] = {1,1,1,1};
    colorBackground[] = {0,0,0,0};
    text = "";
};
class RscEdit_custom: RscText_custom {
    type = 2;
    style = 16;
    colorBackground[] = {0.1,0.1,0.12,1};
    colorText[] = {1,1,1,1};
    sizeEx = 0.04;
};
class RscButton_custom: RscText_custom {
    type = 1;
    style = 2;
    colorBackground[] = {0.3,0.3,0.35,1};
    colorText[] = {1,1,1,1};
    sizeEx = 0.04;
};
class RscListBox_custom: RscText_custom {
    type = 5;
    style = 16;
    colorBackground[] = {0.05,0.05,0.07,1};
    colorText[] = {1,1,1,1};
    colorSelect[] = {0.3,0.3,0.4,1};
    sizeEx = 0.04;
    rowHeight = 0.04;
};
class RscCombo_custom: RscText_custom {
    type = 4;
    style = 16;
    colorBackground[] = {0.1,0.1,0.12,1};
    colorText[] = {1,1,1,1};
    sizeEx = 0.04;
};
class RscCheckbox_custom: RscText_custom {
    type = 7;
    style = 0;
    colorBackground[] = {0.1,0.1,0.12,1};
    colorText[] = {1,1,1,1};
    sizeEx = 0.04;
};
class RscSlider_custom: RscText_custom {
    type = 8;
    style = 0;
    colorBackground[] = {0.3,0.3,0.35,1};
    colorText[] = {1,1,1,1};
};

class a3sql_patch_editor {
    idd = 12300;
    movingEnable = 1;
    enableSimulation = 1;
    enableDisplay = 1;
    onLoad = "uiNamespace setVariable ['a3sql_patch_editor', _this select 0]; call a3sql_patch_fnc_gui_listRules;";

    class controlsBackground {
        class Background: RscText_custom {
            idc = -1;
            x = 0.15;
            y = 0.15;
            w = 0.7;
            h = 0.72;
            colorBackground[] = {0.1, 0.1, 0.12, 0.95};
        };

        class TitleBar: RscText_custom {
            idc = -1;
            text = "A3SQL Patch Editor";
            x = 0.15;
            y = 0.15;
            w = 0.7;
            h = 0.06;
            colorBackground[] = {0.15, 0.15, 0.18, 1};
            style = 2;
            sizeEx = 0.02;
            font = "RobotoCondensedBold";
        };
    };

    class controls {
        // ── Rule List ────────────────────────────────────────────────
        class RuleList: RscListBox_custom {
            idc = 100;
            x = 0.02;
            y = 0.09;
            w = 0.04;
            h = 0.55;
            onLBSelChanged = "_this call a3sql_patch_fnc_gui_editRule";
        };

        // ── Name ─────────────────────────────────────────────────────
        class NameLabel: RscText_custom {
            idc = -1;
            text = "Name:";
            x = 0.02;
            y = 0.66;
            w = 0.08;
            h = 0.04;
            colorText[] = {0.8, 0.8, 0.8, 1};
        };
        class NameEdit: RscEdit_custom {
            idc = 201;
            x = 0.1;
            y = 0.66;
            w = 0.2;
            h = 0.04;
        };

        // ── Active ───────────────────────────────────────────────────
        class ActiveLabel: RscText_custom {
            idc = -1;
            text = "Active:";
            x = 0.32;
            y = 0.66;
            w = 0.06;
            h = 0.04;
            colorText[] = {0.8, 0.8, 0.8, 1};
        };
        class ActiveCheckbox: RscCheckbox_custom {
            idc = 202;
            x = 0.38;
            y = 0.66;
            w = 0.04;
            h = 0.04;
        };

        // ── Priority ─────────────────────────────────────────────────
        class PriorityLabel: RscText_custom {
            idc = -1;
            text = "Priority:";
            x = 0.44;
            y = 0.66;
            w = 0.07;
            h = 0.04;
            colorText[] = {0.8, 0.8, 0.8, 1};
        };
        class PrioritySlider: RscSlider_custom {
            idc = 203;
            x = 0.52;
            y = 0.66;
            w = 0.3;
            h = 0.04;
        };

        // ── Target Type ──────────────────────────────────────────────
        class TargetTypeLabel: RscText_custom {
            idc = -1;
            text = "Target Type:";
            x = 0.02;
            y = 0.72;
            w = 0.1;
            h = 0.04;
            colorText[] = {0.8, 0.8, 0.8, 1};
        };
        class TargetTypeCombo: RscCombo_custom {
            idc = 204;
            x = 0.13;
            y = 0.72;
            w = 0.2;
            h = 0.04;
        };

        // ── Property ─────────────────────────────────────────────────
        class PropertyLabel: RscText_custom {
            idc = -1;
            text = "Property:";
            x = 0.35;
            y = 0.72;
            w = 0.08;
            h = 0.04;
            colorText[] = {0.8, 0.8, 0.8, 1};
        };
        class PropertyEdit: RscEdit_custom {
            idc = 205;
            x = 0.44;
            y = 0.72;
            w = 0.35;
            h = 0.04;
        };

        // ── Operator ─────────────────────────────────────────────────
        class OperatorLabel: RscText_custom {
            idc = -1;
            text = "Operator:";
            x = 0.02;
            y = 0.78;
            w = 0.08;
            h = 0.04;
            colorText[] = {0.8, 0.8, 0.8, 1};
        };
        class OperatorCombo: RscCombo_custom {
            idc = 206;
            x = 0.11;
            y = 0.78;
            w = 0.2;
            h = 0.04;
        };

        // ── Value ────────────────────────────────────────────────────
        class ValueLabel: RscText_custom {
            idc = -1;
            text = "Value:";
            x = 0.35;
            y = 0.78;
            w = 0.06;
            h = 0.04;
            colorText[] = {0.8, 0.8, 0.8, 1};
        };
        class ValueEdit: RscEdit_custom {
            idc = 207;
            x = 0.42;
            y = 0.78;
            w = 0.37;
            h = 0.04;
        };

        // ── Buttons Row ──────────────────────────────────────────────
        class AddBtn: RscButton_custom {
            idc = 300;
            text = "Add";
            x = 0.02;
            y = 0.85;
            w = 0.1;
            h = 0.06;
            action = "call a3sql_patch_fnc_gui_addRule";
        };

        class UpdateBtn: RscButton_custom {
            idc = 301;
            text = "Update";
            x = 0.14;
            y = 0.85;
            w = 0.1;
            h = 0.06;
            action = "call a3sql_patch_fnc_gui_addRule";
        };

        class DeleteBtn: RscButton_custom {
            idc = 302;
            text = "Delete";
            x = 0.26;
            y = 0.85;
            w = 0.1;
            h = 0.06;
            action = "call a3sql_patch_fnc_gui_deleteRule";
        };

        class RefreshBtn: RscButton_custom {
            idc = 303;
            text = "Refresh";
            x = 0.38;
            y = 0.85;
            w = 0.1;
            h = 0.06;
            action = "call a3sql_patch_fnc_gui_listRules";
        };

        class SavePresetBtn: RscButton_custom {
            idc = 304;
            text = "Save Preset";
            x = 0.52;
            y = 0.85;
            w = 0.14;
            h = 0.06;
            action = "call a3sql_patch_fnc_gui_savePreset";
        };

        class LoadPresetBtn: RscButton_custom {
            idc = 305;
            text = "Load Preset";
            x = 0.68;
            y = 0.85;
            w = 0.14;
            h = 0.06;
            action = "call a3sql_patch_fnc_gui_loadPreset";
        };
    };
};
