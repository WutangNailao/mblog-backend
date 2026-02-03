package st.coo.memo.entity;

import com.mybatisflex.annotation.Id;
import com.mybatisflex.annotation.KeyType;
import com.mybatisflex.annotation.Table;
import lombok.Getter;
import lombok.Setter;

import java.io.Serializable;
import java.sql.Timestamp;


@Setter
@Getter
@Table(value = "t_tag")
public class TTag implements Serializable {


    private String name;

    
    private Integer userId;

    
    private Timestamp created;

    
    private Timestamp updated;

    
    private Integer memoCount;

    
    @Id(keyType = KeyType.Auto)
    private Integer id;

}
